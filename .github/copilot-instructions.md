# Copilot instructions for `falkordb-shell`

This repository contains a Rust command-line REPL for FalkorDB.

## Project structure

- `src/main.rs` contains the application logic, REPL command handling, formatting helpers, and unit tests.
- `tests/cli.rs` contains CLI integration tests that exercise the binary against a FalkorDB server.
- `README.md` documents installation, startup, and user-facing shell behavior.
- `help.txt` is the source of truth for `.help` output and help text shown elsewhere.
- `tutorial.yaml` is the source of truth for the tutorial steps embedded into the binary.
- `scripts/build_release_site.py` generates the release website, and `scripts/templates/release_site.html.j2` is its Jinja template.
- `CONTRIBUTING.md` contains release steps and the current backlog/TODO items.

## Coding guidance

- Keep changes small and focused on the CLI and REPL workflow.
- Follow the existing Rust style in `src/main.rs`: explicit enums for shell commands, small helper functions, and straightforward error propagation with `Result`.
- Prefer extending existing helpers over duplicating logic.
- Prefer reusing messages printed over duplicating them.
- Keep user-facing error messages explicit; do not silently ignore failures.
- Preserve the current shell UX unless the task explicitly changes it:
  - dot-prefixed meta commands such as `.help`, `.graph`, `.list`, `.prompt`, and `.stats`
  - query execution against the currently selected graph
  - history stored in `$HOME/.falkordb_shell_history`
- Prefer compile-time embedded assets for built-in content:
  - `help.txt` is included with `include_str!`
  - `tutorial.yaml` is included with `include_str!` and parsed from YAML
- Keep scripting changes aligned with the current Python conventions:
  - use `uv run --script`
  - declare Python dependencies in the script metadata block
  - keep fixed repository file paths hard-coded in the script when they are repository conventions rather than user inputs

## When adding features

- For new meta commands, update all of these together:
  - `HELP`
  - `ShellCommand`
  - `classify_command`
  - `run_shell`
  - unit tests in `src/main.rs`
- For `.tutorial` changes, preserve the current interactive tutorial design unless the task explicitly changes it:
  - switch to the `Tutorial` graph
  - clear the `Tutorial` graph before starting
  - show one tutorial step at a time using framed step cards with progress information
  - blank ENTER executes the current step
  - non-empty input should execute as a normal shell command and then return to the same tutorial step
  - render distinct `Explanation`, `Command`, and `Result` sections
  - keep normal query output formatting unchanged outside tutorial mode
- When changing tutorial content or help text, keep these files in sync:
  - `tutorial.yaml`
  - `help.txt`
  - `README.md` when user-visible behavior changes
- Keep CLI integration tests isolated: use disposable graph names and clean them up after each test.
- Avoid non-TTY CLI tests for `rustyline`-driven interactive flows; those should be validated with real interactive sessions rather than brittle piped-input assertions.
- For output changes, keep formatting deterministic because tests assert exact strings.
- Reuse the existing value-formatting helpers before introducing new rendering paths.
- For release-site changes:
  - keep the HTML in the external Jinja template, not inline in Python
  - use the hard-coded repository paths for `README.md`, `help.txt`, and the template
  - keep `--base-url`, `--repo`, `--tag`, `--version`, and `--release-date` as the script inputs
  - include the repository link and release download links in the generated page

## CI and release automation

- `.github/workflows/tests.yml` is the main workflow file.
- On normal CI runs, use the existing FalkorDB service container and `cargo test --quiet`.
- On `v*` tags, the workflow should:
  - verify the tag matches the version in `Cargo.toml`
  - build release binaries for Linux, macOS, and Windows
  - publish those binaries as GitHub release assets
  - generate and deploy the release site
- Keep the workflow arguments in sync with the release-site script, including the production base URL `https://falkordb-shell.code-maven.com/`.

## Validation

- Run `cargo test --quiet` before you start working on any user request.
- Run `cargo test --quiet`.
- For release-site changes, also run the generator locally with `uv run --script scripts/build_release_site.py ...` and inspect the produced HTML when needed.

## Documentation

- Update `README.md` when adding or changing user-visible behavior or command-line flags.
- As instructions are given to copilot update `.github/copilot-instructions.md` with the description of the features, behavior, coding-style.
