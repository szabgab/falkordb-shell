use clap::Parser;
use falkordb::{FalkorClientBuilder, FalkorConnectionInfo, FalkorValue, QueryResult, SyncGraph};
use rustyline::{CompletionType, Config, DefaultEditor, error::ReadlineError};
use std::{
    collections::HashMap,
    error::Error,
    fmt::Write as _,
    path::{Path, PathBuf},
};

const HISTORY_FILE_NAME: &str = ".falkordb_shell_history";
const PROMPT: &str = "falkordb> ";
const HELP: &str = r#"
.help, - Show this help page.
.exit, .quit - Quit the REPL.
.intro - Introduction to the OpenCyper commands.
"#;
const INTRO: &str = r#"
# Create a `Node` with the `Person` label (type) and the `name` attribute (property)
CREATE (:Person {name: "Alice"})
"#;

#[derive(Debug, Parser)]
struct Args {
    #[arg(long, default_value = "localhost")]
    host: String,
    #[arg(long, default_value_t = 6379)]
    port: u16,
    #[arg(long, default_value = "Shell")]
    graph: String,
}

enum ShellCommand<'a> {
    Empty,
    Exit,
    Help,
    Intro,
    Query(&'a str),
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
    let mut graph = client.select_graph(args.graph);

    println!("Welcome to the interactive FalkorDB shell.");
    println!("Type .help to see the help.");
    let shell_result = run_shell(&mut editor, &mut graph);
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

fn run_shell(editor: &mut DefaultEditor, graph: &mut SyncGraph) -> Result<(), Box<dyn Error>> {
    loop {
        let line = match editor.readline(PROMPT) {
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
            ShellCommand::Help => {
                println!("{HELP}");
            }
            ShellCommand::Intro => {
                println!("{INTRO}");
            }
            ShellCommand::Query(query) => match graph.query(query).execute() {
                Ok(result) => print_result(result),
                Err(error) => println!("ERROR: {error}"),
            },
        }
    }
}

fn classify_command(command: &str) -> ShellCommand<'_> {
    match command {
        "" => ShellCommand::Empty,
        ".exit" | ".quit" => ShellCommand::Exit,
        ".help" => ShellCommand::Help,
        ".intro" => ShellCommand::Intro,
        _ => ShellCommand::Query(command),
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
}
