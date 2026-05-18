use clap::Parser;
use falkordb::{
    FalkorClientBuilder, FalkorConnectionInfo, FalkorSyncClient, FalkorValue, QueryResult,
    SyncGraph,
};
use rustyline::{
    CompletionType, Config, Context, Editor, Helper,
    completion::{Completer, Pair},
    error::ReadlineError,
    highlight::Highlighter,
    hint::Hinter,
    history::DefaultHistory,
    validate::Validator,
};
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
const TUTORIAL_CONTINUE_PROMPT: &str = "Press ENTER to continue tutorial, Ctrl-C to stop> ";
const TUTORIAL_NEXT_STEP_PROMPT: &str = "Press ENTER for next step, Ctrl-C to stop> ";
const TUTORIAL_DIVIDER: &str = "--------------------------------------------------";
const PROJECT_NAME: &str = env!("CARGO_PKG_NAME");
const VERSION: &str = env!("CARGO_PKG_VERSION");
const HELP: &str = include_str!("../help.txt");
const NO_GRAPH_SELECTED: &str =
    "No graph selected. Use `.list` to list available graphs and use `.graph NAME` to select one.";

const TUTORIAL_YAML: &str = include_str!("../tutorial.yaml");
static TUTORIAL: OnceLock<Vec<TutorialStep>> = OnceLock::new();
const DOT_COMMANDS: &[&str] = &[
    ".help",
    ".delete",
    ".exit",
    ".quit",
    ".tutorial",
    ".graph",
    ".list",
    ".prompt",
    ".stats",
];

type ShellEditor = Editor<ShellHelper, DefaultHistory>;

#[derive(Debug, Deserialize)]
struct TutorialStep {
    text: String,
    code: String,
}

#[derive(Default)]
struct ShellHelper;

impl Completer for ShellHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let start = line[..pos]
            .rfind(char::is_whitespace)
            .map(|index| index + 1)
            .unwrap_or(0);
        let prefix = &line[start..pos];
        if !prefix.starts_with('.') {
            return Ok((start, Vec::new()));
        }

        let matches = DOT_COMMANDS
            .iter()
            .filter(|command| command.starts_with(prefix))
            .map(|command| Pair {
                display: (*command).to_string(),
                replacement: (*command).to_string(),
            })
            .collect();
        Ok((start, matches))
    }
}

impl Hinter for ShellHelper {
    type Hint = String;
}

impl Highlighter for ShellHelper {}

impl Validator for ShellHelper {}

impl Helper for ShellHelper {}

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
    Delete(Option<&'a str>),
    Empty,
    Exit,
    Graph(Option<&'a str>),
    Help,
    Tutorial,
    Invalid(&'a str),
    List,
    Prompt,
    Stats,
    Query(&'a str),
}

enum ShellAction {
    Continue,
    Exit,
}

enum TutorialInput {
    ExecuteStep,
    ExecuteCommand(String),
    StopTutorial,
}

#[derive(Clone, Copy)]
enum TutorialPhase {
    ShowingStep,
    AwaitingStep,
    AwaitingNextStep,
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
    println!("Type .tutorial to see the tutorial.");
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

fn setup_editor(history_file: &Path) -> Result<ShellEditor, Box<dyn Error>> {
    let config = Config::builder()
        .max_history_size(1000)?
        .completion_type(CompletionType::List)
        .build();
    let mut editor = ShellEditor::with_config(config)?;
    editor.set_helper(Some(ShellHelper));

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

fn save_history(editor: &mut ShellEditor, history_file: &Path) {
    if let Err(error) = editor.save_history(history_file) {
        eprintln!(
            "ERROR: failed to save history to {}: {error}",
            history_file.display()
        );
    }
}

fn run_shell(
    editor: &mut ShellEditor,
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
        if let ShellAction::Exit =
            handle_command(editor, client, graph, Some(&mut prompt_style), command)?
        {
            return Ok(());
        }
    }
}

fn classify_command(command: &str) -> ShellCommand<'_> {
    if let Some(rest) = command.strip_prefix(".delete") {
        let graph_name = rest.trim();
        return if graph_name.is_empty() {
            ShellCommand::Delete(None)
        } else {
            ShellCommand::Delete(Some(graph_name))
        };
    }

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
        ".tutorial" => ShellCommand::Tutorial,
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

fn run_tutorial(
    editor: &mut ShellEditor,
    client: &FalkorSyncClient,
    graph: &mut Option<SyncGraph>,
) -> Result<ShellAction, Box<dyn Error>> {
    ensure_tutorial_graph(client, graph, true);

    if let Err(error) = clear_graph(
        graph
            .as_mut()
            .expect("tutorial graph should be selected before clearing"),
    ) {
        println!("ERROR: {error}");
        return Ok(ShellAction::Continue);
    }

    for (index, step) in tutorial_steps().iter().enumerate() {
        ensure_tutorial_graph(client, graph, false);
        let mut phase = TutorialPhase::ShowingStep;

        loop {
            if matches!(phase, TutorialPhase::ShowingStep) {
                println!();
                println!(
                    "{}",
                    render_tutorial_step(index + 1, tutorial_steps().len(), step)
                );
            }

            match read_tutorial_input(editor, phase)? {
                TutorialInput::ExecuteStep => {
                    match phase {
                        TutorialPhase::ShowingStep => {
                            ensure_tutorial_graph(client, graph, false);
                            editor.add_history_entry(step.code.as_str())?;
                            if let Err(error) = execute_tutorial_query(
                                graph
                                    .as_mut()
                                    .expect("tutorial graph should be selected before executing"),
                                &step.code,
                            ) {
                                println!("ERROR: {error}");
                                return Ok(ShellAction::Continue);
                            }
                            phase = TutorialPhase::AwaitingNextStep;
                        }
                        TutorialPhase::AwaitingStep => {
                            phase = TutorialPhase::ShowingStep;
                        }
                        TutorialPhase::AwaitingNextStep => break,
                    }
                }
                TutorialInput::ExecuteCommand(command) => {
                    editor.add_history_entry(command.as_str())?;
                    if let ShellAction::Exit =
                        handle_command(editor, client, graph, None, command.as_str())?
                    {
                        return Ok(ShellAction::Exit);
                    }
                    if current_graph_name(graph) != Some(TUTORIAL_GRAPH_NAME) {
                        ensure_tutorial_graph(client, graph, true);
                    }
                    if matches!(phase, TutorialPhase::ShowingStep) {
                        phase = TutorialPhase::AwaitingStep;
                    }
                }
                TutorialInput::StopTutorial => {
                    println!("Tutorial stopped.");
                    return Ok(ShellAction::Continue);
                }
            }
        }
    }

    println!("Tutorial completed.");
    Ok(ShellAction::Continue)
}

fn read_tutorial_input(
    editor: &mut ShellEditor,
    phase: TutorialPhase,
) -> Result<TutorialInput, ReadlineError> {
    loop {
        match editor.readline(tutorial_prompt(phase)) {
            Ok(line) if line.trim().is_empty() => return Ok(TutorialInput::ExecuteStep),
            Ok(line) => return Ok(TutorialInput::ExecuteCommand(line.trim().to_string())),
            Err(ReadlineError::Eof) | Err(ReadlineError::Interrupted) => {
                println!();
                return Ok(TutorialInput::StopTutorial);
            }
            Err(error) => return Err(error),
        }
    }
}

fn handle_command(
    editor: &mut ShellEditor,
    client: &FalkorSyncClient,
    graph: &mut Option<SyncGraph>,
    prompt_style: Option<&mut PromptStyle>,
    command: &str,
) -> Result<ShellAction, Box<dyn Error>> {
    match classify_command(command) {
        ShellCommand::Delete(graph_name) => {
            delete_graph_with_confirmation(editor, client, graph, graph_name)?;
            Ok(ShellAction::Continue)
        }
        ShellCommand::Empty => Ok(ShellAction::Continue),
        ShellCommand::Exit => Ok(ShellAction::Exit),
        ShellCommand::Graph(graph_name) => {
            match graph_name {
                None => match current_graph_name(graph) {
                    Some(graph_name) => println!("{graph_name}"),
                    None => println!("{NO_GRAPH_SELECTED}"),
                },
                Some(graph_name) => {
                    *graph = Some(client.select_graph(graph_name));
                    println!("Switched to graph: {graph_name}");
                }
            }
            Ok(ShellAction::Continue)
        }
        ShellCommand::Help => {
            println!("{HELP}");
            Ok(ShellAction::Continue)
        }
        ShellCommand::Tutorial => run_tutorial(editor, client, graph),
        ShellCommand::Invalid(command) => {
            println!("ERROR: unknown command: {command}");
            Ok(ShellAction::Continue)
        }
        ShellCommand::List => {
            match client.list_graphs() {
                Ok(graphs) if graphs.is_empty() => println!("No graphs found."),
                Ok(graphs) => {
                    let current_graph = current_graph_name(graph);
                    for graph_name in graphs {
                        let stats = match graph_counts(client, &graph_name) {
                            Ok((nodes, edges)) => format!(" (nodes: {nodes}, edges: {edges})"),
                            Err(error) => format!(" (ERROR: {error})"),
                        };
                        if Some(graph_name.as_str()) == current_graph {
                            println!("{graph_name}{stats} *");
                        } else {
                            println!("{graph_name}{stats}");
                        }
                    }
                }
                Err(error) => println!("ERROR: {error}"),
            }
            Ok(ShellAction::Continue)
        }
        ShellCommand::Prompt => {
            match prompt_style {
                Some(prompt_style) => {
                    *prompt_style = match *prompt_style {
                        PromptStyle::Plain => PromptStyle::GraphName,
                        PromptStyle::GraphName => PromptStyle::Plain,
                    };
                }
                None => println!("Prompt style can only be changed from the main shell prompt."),
            }
            Ok(ShellAction::Continue)
        }
        ShellCommand::Stats => {
            match graph {
                Some(graph) => match print_stats(graph) {
                    Ok(()) => {}
                    Err(error) => println!("ERROR: {error}"),
                },
                None => println!("ERROR: {NO_GRAPH_SELECTED}"),
            }
            Ok(ShellAction::Continue)
        }
        ShellCommand::Query(query) => {
            match graph {
                Some(graph) => match execute_query(graph, query) {
                    Ok(()) => {}
                    Err(error) => println!("ERROR: {error}"),
                },
                None => println!("ERROR: {NO_GRAPH_SELECTED}"),
            }
            Ok(ShellAction::Continue)
        }
    }
}

fn ensure_tutorial_graph(
    client: &FalkorSyncClient,
    graph: &mut Option<SyncGraph>,
    announce_switch: bool,
) {
    if current_graph_name(graph) == Some(TUTORIAL_GRAPH_NAME) {
        return;
    }

    *graph = Some(client.select_graph(TUTORIAL_GRAPH_NAME));
    if announce_switch {
        println!("Switched to graph: {TUTORIAL_GRAPH_NAME}");
    }
}

fn delete_graph_with_confirmation(
    editor: &mut ShellEditor,
    client: &FalkorSyncClient,
    current_graph: &mut Option<SyncGraph>,
    graph_name: Option<&str>,
) -> Result<(), Box<dyn Error>> {
    let Some(graph_name) = graph_name else {
        println!("ERROR: usage: .delete NAME");
        return Ok(());
    };

    let graphs = client.list_graphs()?;
    if !graphs.iter().any(|name| name == graph_name) {
        println!("ERROR: graph not found: {graph_name}");
        return Ok(());
    }

    let (node_count, edge_count) = graph_counts(client, graph_name)?;
    let prompt = format!(
        "Delete graph '{graph_name}' with {node_count} nodes and {edge_count} edges? [y/N] "
    );
    let confirmed = match editor.readline(&prompt) {
        Ok(answer) => is_confirmation(answer.trim()),
        Err(ReadlineError::Eof) | Err(ReadlineError::Interrupted) => {
            println!();
            false
        }
        Err(error) => return Err(error.into()),
    };

    if !confirmed {
        println!("Deletion cancelled.");
        return Ok(());
    }

    let mut graph = client.select_graph(graph_name);
    graph.delete()?;
    if current_graph_name(current_graph) == Some(graph_name) {
        *current_graph = None;
    }
    println!("Deleted graph: {graph_name}");
    Ok(())
}

fn graph_counts(client: &FalkorSyncClient, graph_name: &str) -> Result<(i64, i64), String> {
    let mut graph = client.select_graph(graph_name);
    let node_count = query_single_i64(&mut graph, "MATCH (n) RETURN count(n)")?;
    let edge_count = query_single_i64(&mut graph, "MATCH ()-[r]->() RETURN count(r)")?;
    Ok((node_count, edge_count))
}

fn is_confirmation(answer: &str) -> bool {
    matches!(answer, "y" | "Y" | "yes" | "YES" | "Yes")
}

fn render_tutorial_step(step_number: usize, total_steps: usize, step: &TutorialStep) -> String {
    let mut rendered = String::new();
    rendered.push_str(TUTORIAL_DIVIDER);
    rendered.push('\n');
    let _ = writeln!(&mut rendered, "Tutorial {step_number}/{total_steps}");
    let _ = writeln!(&mut rendered, "Graph: {TUTORIAL_GRAPH_NAME}");
    rendered.push_str("\nExplanation\n");
    append_indented_block(&mut rendered, &step.text, "  ");
    rendered.push_str("\n\nCommand\n");
    append_indented_block(&mut rendered, &step.code, "  ");
    rendered.push('\n');
    rendered.push_str(TUTORIAL_DIVIDER);

    rendered
}

fn tutorial_prompt(phase: TutorialPhase) -> &'static str {
    match phase {
        TutorialPhase::ShowingStep => TUTORIAL_STEP_PROMPT,
        TutorialPhase::AwaitingStep => TUTORIAL_CONTINUE_PROMPT,
        TutorialPhase::AwaitingNextStep => TUTORIAL_NEXT_STEP_PROMPT,
    }
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

fn execute_tutorial_query(graph: &mut SyncGraph, query: &str) -> Result<(), String> {
    let result = graph
        .query(query)
        .execute()
        .map_err(|error| error.to_string())?;
    print_tutorial_result(result);
    Ok(())
}

fn clear_graph(graph: &mut SyncGraph) -> Result<(), String> {
    graph
        .query("MATCH (n) DETACH DELETE n")
        .execute()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn print_tutorial_result(result: QueryResult<falkordb::LazyResultSet<'_>>) {
    let lines = format_result_lines(&result.header, result.data);
    println!("Result");
    if lines.is_empty() {
        println!("  OK");
        return;
    }

    for line in lines {
        println!("  {line}");
    }
}

fn append_indented_block(rendered: &mut String, text: &str, prefix: &str) {
    for (index, line) in text.lines().enumerate() {
        if index > 0 {
            rendered.push('\n');
        }
        let _ = write!(rendered, "{prefix}{line}");
    }
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
    use rustyline::history::DefaultHistory;

    #[test]
    fn classifies_meta_commands() {
        assert!(matches!(
            classify_command(".delete Demo"),
            ShellCommand::Delete(Some("Demo"))
        ));
        assert!(matches!(
            classify_command(".delete"),
            ShellCommand::Delete(None)
        ));
        assert!(matches!(classify_command(""), ShellCommand::Empty));
        assert!(matches!(classify_command(".help"), ShellCommand::Help));
        assert!(matches!(
            classify_command(".tutorial"),
            ShellCommand::Tutorial
        ));
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
        let tutorial = render_tutorial_step(1, tutorial_steps().len(), &tutorial_steps()[0]);

        assert!(tutorial.contains(TUTORIAL_DIVIDER));
        assert!(tutorial.contains(&format!("Tutorial 1/{}", tutorial_steps().len())));
        assert!(tutorial.contains("Explanation"));
        assert!(tutorial.contains(
            "  Create a `Node` with the `Person` `label` (type) and the `name` attribute (property)."
        ));
        assert!(tutorial.contains("Command"));
        assert!(tutorial.contains(r#"  CREATE (:Person {name: "Alice"})"#));
    }

    #[test]
    fn uses_distinct_tutorial_prompts() {
        assert_eq!(
            tutorial_prompt(TutorialPhase::ShowingStep),
            "Press ENTER to run, Ctrl-C to stop> "
        );
        assert_eq!(
            tutorial_prompt(TutorialPhase::AwaitingStep),
            "Press ENTER to continue tutorial, Ctrl-C to stop> "
        );
        assert_eq!(
            tutorial_prompt(TutorialPhase::AwaitingNextStep),
            "Press ENTER for next step, Ctrl-C to stop> "
        );
    }

    #[test]
    fn parses_embedded_tutorial_yaml() {
        let tutorial = tutorial_steps();

        assert_eq!(tutorial.len(), 15);
        assert_eq!(
            tutorial.first().map(|step| step.code.as_str()),
            Some(r#"CREATE (:Person {name: "Alice"})"#)
        );
    }

    #[test]
    fn completes_dot_commands() {
        let helper = ShellHelper;
        let history = DefaultHistory::new();
        let context = Context::new(&history);
        let (start, candidates) = helper.complete(".t", 2, &context).unwrap();

        assert_eq!(start, 0);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].replacement, ".tutorial");
    }

    #[test]
    fn completes_delete_command() {
        let helper = ShellHelper;
        let history = DefaultHistory::new();
        let context = Context::new(&history);
        let (_start, candidates) = helper.complete(".d", 2, &context).unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].replacement, ".delete");
    }

    #[test]
    fn recognizes_confirmation_answers() {
        assert!(is_confirmation("y"));
        assert!(is_confirmation("Yes"));
        assert!(is_confirmation("YES"));
        assert!(!is_confirmation(""));
        assert!(!is_confirmation("n"));
    }
}
