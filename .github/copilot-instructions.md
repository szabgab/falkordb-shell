# Copilot instructions for `falkordb-shell`

This repository contains a Rust command-line REPL for FalkorDB.

## Project structure

- `src/main.rs` contains the application logic, REPL command handling, formatting helpers, and unit tests.
- `tests/cli.rs` contains CLI integration tests that exercise the binary against a FalkorDB server.
- `README.md` documents installation, startup, and user-facing shell behavior.
- `CONTRIBUTING.md` contains release steps and the current backlog/TODO items.

## Coding guidance

- Keep changes small and focused on the CLI and REPL workflow.
- Follow the existing Rust style in `src/main.rs`: explicit enums for shell commands, small helper functions, and straightforward error propagation with `Result`.
- Prefer extending existing helpers over duplicating logic.
- Keep user-facing error messages explicit; do not silently ignore failures.
- Preserve the current shell UX unless the task explicitly changes it:
  - dot-prefixed meta commands such as `.help`, `.graph`, `.list`, `.prompt`, and `.stats`
  - query execution against the currently selected graph
  - history stored in `$HOME/.falkordb_shell_history`

## When adding features

- For new meta commands, update all of these together:
  - `HELP`
  - `ShellCommand`
  - `classify_command`
  - `run_shell`
  - unit tests in `src/main.rs`
- Keep CLI integration tests isolated: use disposable graph names and clean them up after each test.
- For output changes, keep formatting deterministic because tests assert exact strings.
- Reuse the existing value-formatting helpers before introducing new rendering paths.

## Validation

- Run `cargo test --quiet`.

## Documentation

- Update `README.md` when adding or changing user-visible behavior or command-line flags.
- As instructions are given to copilot update `.github/copilot-instructions.md` with the description of the features, behavior, coding-style.

