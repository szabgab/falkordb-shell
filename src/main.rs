use clap::Parser;
use falkordb::{
    FalkorClientBuilder, FalkorConnectionInfo, FalkorSyncClient, FalkorValue, QueryResult,
    SyncGraph,
};
use rustyline::{CompletionType, Config, DefaultEditor, error::ReadlineError};
use serde::Deserialize;
use std::{
    collections::HashMap,
    error::Error,
    fmt::Write as _,
    path::{Path, PathBuf},
    sync::OnceLock,
};

const HISTORY_FILE_NAME: &str = ".falkordb_shell_history";
const PLAIN_PROMPT: &str = "falkordb> ";
const TUTORIAL_GRAPH_NAME: &str = "Tutorial";
const TUTORIAL_STEP_PROMPT: &str = "Press ENTER to run, Ctrl-C to stop> ";
const PROJECT_NAME: &str = env!("CARGO_PKG_NAME");
const VERSION: &str = env!("CARGO_PKG_VERSION");
const HELP: &str = include_str!("../help.txt");
const NO_GRAPH_SELECTED: &str =
    "No graph selected. Use `.list` to list available graphs and use `.graph NAME` to select one.";

const TUTORIAL_YAML: &str = include_str!("../tutorial.yaml");
static TUTORIAL: OnceLock<Vec<TutorialStep>> = OnceLock::new();

#[derive(Debug, Deserialize)]
struct TutorialStep {
    text: String,
    code: String,
}

#[derive(Debug, Parser)]
#[command(name = PROJECT_NAME, version = VERSION)]
struct Args {
    #[arg(long, default_value = "localhost")]
    host: String,
    #[arg(long, default_value_t = 6379)]
    port: u16,
    #[arg(long)]
    graph: Option<String>,
}

enum ShellCommand<'a> {
    Empty,
    Exit,
    Graph(Option<&'a str>),
    Help,
    Intro,
    Invalid(&'a str),
    List,
    Prompt,
    Stats,
    Query(&'a str),
}

#[derive(Clone, Copy)]
enum PromptStyle {
    Plain,
    GraphName,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let history_file = history_file_path();
    let mut editor = setup_editor(&history_file)?;

    let connection_info: FalkorConnectionInfo =
        format!("falkor://{}:{}", args.host, args.port).try_into()?;
    let client = FalkorClientBuilder::new()
        .with_connection_info(connection_info)
        .build()?;
    let mut graph = args.graph.map(|graph_name| client.select_graph(graph_name));

    println!("Welcome to the interactive FalkorDB shell v{VERSION}.");
    println!("Type .help to see the help.");
    let shell_result = run_shell(&mut editor, &client, &mut graph);
    save_history(&mut editor, &history_file);
    shell_result
}

fn history_file_path() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(HISTORY_FILE_NAME)
}

fn setup_editor(history_file: &Path) -> Result<DefaultEditor, Box<dyn Error>> {
    let config = Config::builder()
        .max_history_size(1000)?
        .completion_type(CompletionType::List)
        .build();
    let mut editor = DefaultEditor::with_config(config)?;

    if history_file.exists() {
        if let Err(error) = editor.load_history(history_file) {
            eprintln!(
                "ERROR: failed to load history from {}: {error}",
                history_file.display()
            );
        }
    }

    Ok(editor)
}

fn save_history(editor: &mut DefaultEditor, history_file: &Path) {
    if let Err(error) = editor.save_history(history_file) {
        eprintln!(
            "ERROR: failed to save history to {}: {error}",
            history_file.display()
        );
    }
}

fn run_shell(
    editor: &mut DefaultEditor,
    client: &FalkorSyncClient,
    graph: &mut Option<SyncGraph>,
) -> Result<(), Box<dyn Error>> {
    let mut prompt_style = PromptStyle::GraphName;

    loop {
        let prompt = format_prompt(prompt_style, current_graph_name(graph));
        let line = match editor.readline(&prompt) {
            Ok(line) => line,
            Err(ReadlineError::Eof) => {
                println!();
                return Ok(());
            }
            Err(ReadlineError::Interrupted) => {
                println!();
                continue;
            }
            Err(error) => return Err(error.into()),
        };

        let command = line.trim();
        if command.is_empty() {
            continue;
        }

        editor.add_history_entry(command)?;
        match classify_command(command) {
            ShellCommand::Empty => continue,
            ShellCommand::Exit => return Ok(()),
            ShellCommand::Graph(graph_name) => match graph_name {
                None => match current_graph_name(graph) {
                    Some(graph_name) => println!("{graph_name}"),
                    None => println!("{NO_GRAPH_SELECTED}"),
                },
                Some(graph_name) => {
                    *graph = Some(client.select_graph(graph_name));
                    println!("Switched to graph: {graph_name}");
                }
            },
            ShellCommand::Help => {
                println!("{HELP}");
            }
            ShellCommand::Intro => {
                run_intro(editor, client, graph)?;
            }
            ShellCommand::Invalid(command) => {
                println!("ERROR: unknown command: {command}");
            }
            ShellCommand::List => match client.list_graphs() {
                Ok(graphs) if graphs.is_empty() => println!("No graphs found."),
                Ok(graphs) => {
                    let current_graph = current_graph_name(graph);
                    for graph_name in graphs {
                        if Some(graph_name.as_str()) == current_graph {
                            println!("{graph_name} *");
                        } else {
                            println!("{graph_name}");
                        }
                    }
                }
                Err(error) => println!("ERROR: {error}"),
            },
            ShellCommand::Prompt => {
                prompt_style = match prompt_style {
                    PromptStyle::Plain => PromptStyle::GraphName,
                    PromptStyle::GraphName => PromptStyle::Plain,
                };
            }
            ShellCommand::Stats => match graph {
                Some(graph) => match print_stats(graph) {
                    Ok(()) => {}
                    Err(error) => println!("ERROR: {error}"),
                },
                None => println!("ERROR: {NO_GRAPH_SELECTED}"),
            },
            ShellCommand::Query(query) => match graph {
                Some(graph) => match execute_query(graph, query) {
                    Ok(()) => {}
                    Err(error) => println!("ERROR: {error}"),
                },
                None => println!("ERROR: {NO_GRAPH_SELECTED}"),
            },
        }
    }
}

fn classify_command(command: &str) -> ShellCommand<'_> {
    if let Some(rest) = command.strip_prefix(".graph") {
        let graph_name = rest.trim();
        return if graph_name.is_empty() {
            ShellCommand::Graph(None)
        } else {
            ShellCommand::Graph(Some(graph_name))
        };
    }

    match command {
        "" => ShellCommand::Empty,
        ".exit" | ".quit" => ShellCommand::Exit,
        ".help" => ShellCommand::Help,
        ".intro" => ShellCommand::Intro,
        ".list" => ShellCommand::List,
        ".prompt" => ShellCommand::Prompt,
        ".stats" => ShellCommand::Stats,
        _ if command.starts_with('.') => ShellCommand::Invalid(command),
        _ => ShellCommand::Query(command),
    }
}

fn current_graph_name(graph: &Option<SyncGraph>) -> Option<&str> {
    graph.as_ref().map(SyncGraph::graph_name)
}

fn tutorial_steps() -> &'static [TutorialStep] {
    TUTORIAL
        .get_or_init(|| {
            serde_yaml::from_str(TUTORIAL_YAML).expect("embedded tutorial.yaml must be valid")
        })
        .as_slice()
}

fn run_intro(
    editor: &mut DefaultEditor,
    client: &FalkorSyncClient,
    graph: &mut Option<SyncGraph>,
) -> Result<(), Box<dyn Error>> {
    *graph = Some(client.select_graph(TUTORIAL_GRAPH_NAME));
    println!("Switched to graph: {TUTORIAL_GRAPH_NAME}");

    let Some(tutorial_graph) = graph.as_mut() else {
        unreachable!("tutorial graph was just selected");
    };

    if let Err(error) = clear_graph(tutorial_graph) {
        println!("ERROR: {error}");
        return Ok(());
    }

    for (index, step) in tutorial_steps().iter().enumerate() {
        println!();
        println!("{}", render_tutorial_step(index + 1, step));

        if !wait_for_tutorial_step(editor)? {
            println!("Tutorial stopped.");
            return Ok(());
        }

        if let Err(error) = execute_query(tutorial_graph, &step.code) {
            println!("ERROR: {error}");
            return Ok(());
        }
    }

    println!("Tutorial completed.");
    Ok(())
}

fn wait_for_tutorial_step(editor: &mut DefaultEditor) -> Result<bool, ReadlineError> {
    loop {
        match editor.readline(TUTORIAL_STEP_PROMPT) {
            Ok(line) if line.trim().is_empty() => return Ok(true),
            Ok(_) => println!("Press ENTER to run the tutorial command."),
            Err(ReadlineError::Eof) | Err(ReadlineError::Interrupted) => {
                println!();
                return Ok(false);
            }
            Err(error) => return Err(error),
        }
    }
}

fn render_tutorial_step(step_number: usize, step: &TutorialStep) -> String {
    let mut rendered = String::new();
    let _ = write!(
        &mut rendered,
        "{}. {}",
        step_number,
        step.text.replace('\n', "\n   ")
    );
    let _ = write!(&mut rendered, "\n   {}", step.code);

    rendered
}

fn format_prompt(prompt_style: PromptStyle, graph_name: Option<&str>) -> String {
    match prompt_style {
        PromptStyle::Plain => PLAIN_PROMPT.to_string(),
        PromptStyle::GraphName => match graph_name {
            Some(graph_name) => format!("falkordb ({graph_name})> "),
            None => PLAIN_PROMPT.to_string(),
        },
    }
}

fn print_result(result: QueryResult<falkordb::LazyResultSet<'_>>) {
    let lines = format_result_lines(&result.header, result.data);
    if lines.is_empty() {
        println!("OK");
        return;
    }

    for line in lines {
        println!("{line}");
    }
}

fn execute_query(graph: &mut SyncGraph, query: &str) -> Result<(), String> {
    let result = graph
        .query(query)
        .execute()
        .map_err(|error| error.to_string())?;
    print_result(result);
    Ok(())
}

fn clear_graph(graph: &mut SyncGraph) -> Result<(), String> {
    graph
        .query("MATCH (n) DETACH DELETE n")
        .execute()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn print_stats(graph: &mut SyncGraph) -> Result<(), String> {
    let total_nodes = query_single_i64(graph, "MATCH (n) RETURN count(n)")?;
    let total_edges = query_single_i64(graph, "MATCH ()-[r]->() RETURN count(r)")?;
    let node_types = query_named_counts(
        graph,
        "MATCH (n)
         UNWIND CASE
             WHEN size(labels(n)) = 0 THEN ['(unlabeled)']
             ELSE labels(n)
         END AS label
         RETURN label, count(*) AS count
         ORDER BY label",
    )?;
    let edge_types = query_named_counts(
        graph,
        "MATCH ()-[r]->()
         RETURN type(r), count(*) AS count
         ORDER BY type(r)",
    )?;

    println!("Total nodes: {total_nodes}");
    println!("Total edges: {total_edges}");
    print_named_counts("Node types", &node_types);
    print_named_counts("Edge types", &edge_types);

    Ok(())
}

fn query_single_i64(graph: &mut SyncGraph, query: &str) -> Result<i64, String> {
    let result = graph
        .query(query)
        .execute()
        .map_err(|error| error.to_string())?;
    let mut rows = result.data;
    let row = rows
        .next()
        .ok_or_else(|| format!("query returned no rows: {query}"))?;
    let value = row
        .into_iter()
        .next()
        .ok_or_else(|| format!("query returned no columns: {query}"))?;
    value
        .to_i64()
        .ok_or_else(|| format!("query did not return an integer: {query}"))
}

fn query_named_counts(graph: &mut SyncGraph, query: &str) -> Result<Vec<(String, i64)>, String> {
    let result = graph
        .query(query)
        .execute()
        .map_err(|error| error.to_string())?;
    let mut counts = Vec::new();

    for row in result.data {
        let [name, count]: [FalkorValue; 2] = row
            .try_into()
            .map_err(|_| format!("query did not return two columns: {query}"))?;

        let name = match name {
            FalkorValue::String(name) => name,
            other => {
                return Err(format!(
                    "query did not return a string label/type: {}",
                    format_value(&other)
                ));
            }
        };
        let count = count
            .to_i64()
            .ok_or_else(|| format!("query did not return an integer count: {query}"))?;
        counts.push((name, count));
    }

    Ok(counts)
}

fn print_named_counts(title: &str, counts: &[(String, i64)]) {
    println!("{title}:");
    if counts.is_empty() {
        println!("  (none)");
        return;
    }

    for (name, count) in counts {
        println!("  {name}: {count}");
    }
}

fn format_result_lines(headers: &[String], result_set: falkordb::LazyResultSet<'_>) -> Vec<String> {
    let mut lines = Vec::new();

    for row in result_set {
        for (index, item) in row.into_iter().enumerate() {
            lines.extend(format_item_lines(
                headers.get(index).map(String::as_str),
                item,
            ));
        }
    }

    lines
}

fn format_item_lines(alias: Option<&str>, item: FalkorValue) -> Vec<String> {
    let alias = alias.unwrap_or_default();

    match item {
        FalkorValue::Node(node) => vec![
            "Node".to_string(),
            format!("id: {}", node.entity_id),
            format!("alias: {alias}"),
            format!("labels: {}", format_string_list(&node.labels)),
            format!("properties: {}", format_properties(&node.properties)),
        ],
        FalkorValue::Edge(edge) => vec![
            "Edge".to_string(),
            format!("id: {}", edge.entity_id),
            format!("alias: {alias}"),
            format!("relationship_type: {}", edge.relationship_type),
            format!("source: {}", edge.src_node_id),
            format!("destination: {}", edge.dst_node_id),
            format!("properties: {}", format_properties(&edge.properties)),
        ],
        value => vec![
            value_type_name(&value).to_string(),
            format!("alias: {alias}"),
            format!("value: {}", format_value(&value)),
        ],
    }
}

fn value_type_name(value: &FalkorValue) -> &'static str {
    match value {
        FalkorValue::Node(_) => "Node",
        FalkorValue::Edge(_) => "Edge",
        FalkorValue::Array(_) => "Array",
        FalkorValue::Map(_) => "Map",
        FalkorValue::Vec32(_) => "Vec32",
        FalkorValue::String(_) => "String",
        FalkorValue::Bool(_) => "Bool",
        FalkorValue::I64(_) => "I64",
        FalkorValue::F64(_) => "F64",
        FalkorValue::Point(_) => "Point",
        FalkorValue::Path(_) => "Path",
        FalkorValue::None => "None",
        FalkorValue::Unparseable(_) => "Unparseable",
    }
}

fn format_properties(properties: &HashMap<String, FalkorValue>) -> String {
    format_map_entries(properties.iter().map(|(key, value)| (key.as_str(), value)))
}

fn format_string_list(values: &[String]) -> String {
    let quoted = values
        .iter()
        .map(|value| format!("\"{value}\""))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{quoted}]")
}

fn format_map_entries<'a, I>(entries: I) -> String
where
    I: IntoIterator<Item = (&'a str, &'a FalkorValue)>,
{
    let mut entries = entries
        .into_iter()
        .map(|(key, value)| (key, format_value(value)))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(right.0));

    let joined = entries
        .into_iter()
        .map(|(key, value)| format!("{key}: {value}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{{{joined}}}")
}

fn format_value(value: &FalkorValue) -> String {
    match value {
        FalkorValue::Node(node) => format!(
            "Node{{id: {}, labels: {}, properties: {}}}",
            node.entity_id,
            format_string_list(&node.labels),
            format_properties(&node.properties)
        ),
        FalkorValue::Edge(edge) => format!(
            "Edge{{id: {}, relationship_type: {}, source: {}, destination: {}, properties: {}}}",
            edge.entity_id,
            edge.relationship_type,
            edge.src_node_id,
            edge.dst_node_id,
            format_properties(&edge.properties)
        ),
        FalkorValue::Array(values) => {
            let rendered = values
                .iter()
                .map(format_value)
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{rendered}]")
        }
        FalkorValue::Map(values) => {
            format_map_entries(values.iter().map(|(key, value)| (key.as_str(), value)))
        }
        FalkorValue::Vec32(vector) => format!("{vector:?}"),
        FalkorValue::String(text) => format!("\"{text}\""),
        FalkorValue::Bool(value) => value.to_string(),
        FalkorValue::I64(value) => value.to_string(),
        FalkorValue::F64(value) => value.to_string(),
        FalkorValue::Point(point) => format!("{point:?}"),
        FalkorValue::Path(path) => format!("{path:?}"),
        FalkorValue::None => "null".to_string(),
        FalkorValue::Unparseable(error) => {
            let mut rendered = String::from("unparseable(");
            let _ = write!(&mut rendered, "\"{error}\"");
            rendered.push(')');
            rendered
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use falkordb::Node;

    #[test]
    fn classifies_meta_commands() {
        assert!(matches!(classify_command(""), ShellCommand::Empty));
        assert!(matches!(classify_command(".help"), ShellCommand::Help));
        assert!(matches!(classify_command(".intro"), ShellCommand::Intro));
        assert!(matches!(classify_command(".list"), ShellCommand::List));
        assert!(matches!(classify_command(".prompt"), ShellCommand::Prompt));
        assert!(matches!(classify_command(".stats"), ShellCommand::Stats));
        assert!(matches!(
            classify_command(".bogus"),
            ShellCommand::Invalid(".bogus")
        ));
        assert!(matches!(
            classify_command(".graph"),
            ShellCommand::Graph(None)
        ));
        assert!(matches!(
            classify_command(".graph demo"),
            ShellCommand::Graph(Some("demo"))
        ));
        assert!(matches!(classify_command(".quit"), ShellCommand::Exit));
        assert!(matches!(
            classify_command("RETURN 1"),
            ShellCommand::Query("RETURN 1")
        ));
    }

    #[test]
    fn formats_node_output_with_alias() {
        let mut properties = HashMap::new();
        properties.insert("name".to_string(), FalkorValue::String("Alice".to_string()));
        properties.insert("age".to_string(), FalkorValue::I64(23));

        let lines = format_item_lines(
            Some("person"),
            FalkorValue::Node(Node {
                entity_id: 7,
                labels: vec!["Person".to_string()],
                properties,
            }),
        );

        assert_eq!(
            lines,
            vec![
                "Node",
                "id: 7",
                "alias: person",
                "labels: [\"Person\"]",
                "properties: {age: 23, name: \"Alice\"}",
            ]
        );
    }

    #[test]
    fn formats_scalar_output() {
        let lines = format_item_lines(Some("n"), FalkorValue::I64(42));

        assert_eq!(lines, vec!["I64", "alias: n", "value: 42"]);
    }

    #[test]
    fn formats_prompt() {
        assert_eq!(
            format_prompt(PromptStyle::Plain, Some("TestGraph")),
            "falkordb> "
        );
        assert_eq!(
            format_prompt(PromptStyle::GraphName, Some("TestGraph")),
            "falkordb (TestGraph)> "
        );
        assert_eq!(format_prompt(PromptStyle::GraphName, None), "falkordb> ");
    }

    #[test]
    fn renders_embedded_tutorial() {
        let tutorial = render_tutorial_step(1, &tutorial_steps()[0]);

        assert!(tutorial.contains(
            "1. Create a `Node` with the `Person` label (type) and the `name` attribute (property)"
        ));
        assert!(tutorial.contains(r#"   CREATE (:Person {name: "Alice"})"#));
    }

    #[test]
    fn parses_embedded_tutorial_yaml() {
        let tutorial = tutorial_steps();

        assert_eq!(tutorial.len(), 12);
        assert_eq!(
            tutorial.first().map(|step| step.code.as_str()),
            Some(r#"CREATE (:Person {name: "Alice"})"#)
        );
    }
}
