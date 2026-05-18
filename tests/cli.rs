use falkordb::{FalkorClientBuilder, FalkorConnectionInfo, FalkorSyncClient};
use std::{
    env,
    error::Error,
    io::{Error as IoError, Write},
    process::{self, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

struct GraphCleanup {
    graph_name: String,
}

impl GraphCleanup {
    fn new(graph_name: String) -> Self {
        Self { graph_name }
    }
}

impl Drop for GraphCleanup {
    fn drop(&mut self) {
        if let Ok(client) = test_client() {
            let _ = client.select_graph(&self.graph_name).delete();
        }
    }
}

#[test]
fn shell_executes_query_in_random_graph() -> Result<(), Box<dyn Error>> {
    let Some(output) = run_shell_test("RETURN 1\n.exit\n")? else {
        return Ok(());
    };

    let stdout = String::from_utf8(output.stdout)?;
    assert!(
        output.status.success(),
        "shell exited unsuccessfully\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("Welcome to the interactive FalkorDB shell"));
    assert!(stdout.contains("I64"));
    assert!(stdout.contains("value: 1"));

    Ok(())
}

#[test]
fn shell_lists_and_reports_stats_for_random_graph() -> Result<(), Box<dyn Error>> {
    let graph_name = unique_graph_name();
    let _cleanup = GraphCleanup::new(graph_name.clone());
    let Some(output) = run_shell(
        &graph_name,
        r#"CREATE (:Person {name: "Alice"})
.list
.stats
.exit
"#,
    )?
    else {
        return Ok(());
    };

    let stdout = String::from_utf8(output.stdout)?;
    assert!(
        output.status.success(),
        "shell exited unsuccessfully\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains(&format!("{graph_name} (nodes: 1, edges: 0) *")));
    assert!(stdout.contains("Total nodes: 1"));
    assert!(stdout.contains("Total edges: 0"));
    assert!(stdout.contains("Node types:\n  Person: 1"));

    Ok(())
}

fn run_shell_test(input: &str) -> Result<Option<process::Output>, Box<dyn Error>> {
    let graph_name = unique_graph_name();
    let _cleanup = GraphCleanup::new(graph_name.clone());
    run_shell(&graph_name, input)
}

fn run_shell(graph_name: &str, input: &str) -> Result<Option<process::Output>, Box<dyn Error>> {
    if test_client().is_err() {
        eprintln!("Skipping CLI integration test: FalkorDB is not available.");
        return Ok(None);
    }

    let host = test_host();
    let port = test_port().to_string();
    let mut child = Command::new(env!("CARGO_BIN_EXE_falkordb-shell"))
        .args(["--host", &host, "--port", &port, "--graph", graph_name])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    child
        .stdin
        .as_mut()
        .ok_or_else(|| IoError::other("failed to open shell stdin"))?
        .write_all(input.as_bytes())?;

    Ok(Some(child.wait_with_output()?))
}

fn test_client() -> Result<FalkorSyncClient, Box<dyn Error>> {
    let connection_info: FalkorConnectionInfo =
        format!("falkor://{}:{}", test_host(), test_port()).try_into()?;
    Ok(FalkorClientBuilder::new()
        .with_connection_info(connection_info)
        .build()?)
}

fn test_host() -> String {
    env::var("FALKORDB_HOST").unwrap_or_else(|_| "localhost".to_string())
}

fn test_port() -> u16 {
    env::var("FALKORDB_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(6379)
}

fn unique_graph_name() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before unix epoch")
        .as_nanos();
    let counter = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);

    format!(
        "falkordb_shell_test_{}_{}_{}",
        process::id(),
        nanos,
        counter
    )
}
