Title: feat(server): --host/STROM_HOST to choose the bind address

## Problem

The server always binds `0.0.0.0` (both in native GUI mode and headless), with no way to restrict it. A desktop setup where only a GUI on the same machine talks to Strom wants it reachable on loopback only, not on every interface of the machine.

## Change

- `server.host` in the config file and `STROM_SERVER_HOST` (next to `server.port` / `STROM_SERVER_PORT`).
- `--host` / `STROM_HOST` on the command line (next to `--port` / `STROM_PORT`), which wins over the config file, as for the port.
- Both bind sites use `SocketAddr::new(config.host, config.port)`.
- Default unchanged: `0.0.0.0`, so Docker and network access keep working.
- `.strom.toml.example` and the README configuration block name the option.

## Evidence

- Default stays all interfaces: `config::tests::test_from_figment_defaults` asserts `0.0.0.0`.
- Config file: `test_from_figment_config_file` reads `host = "127.0.0.1"`.
- Env: `test_from_figment_host_from_env` reads `STROM_SERVER_HOST=::1`.
- CLI: ran `strom --headless --host 127.0.0.1 --port 18765`; `ss -ltn` showed `127.0.0.1:18765` only, `GET /api/flows` on 127.0.0.1 answered 200.

## Tests

Ran on Linux (Fedora 44, GStreamer 1.28, x86_64), on current `main` (4479d8c):
- `cargo test --lib config::`: 27 passed.
- `cargo build` and the manual run above.
- `cargo fmt --all --check`: clean; `cargo clippy --all-targets`: no warnings in the touched files (three pre-existing warnings from a newer clippy in untouched files).

Not run: the native GUI mode with `--host`.

## For the reviewer

The native GUI connects to `localhost:{port}`. With `--host` set to a non-loopback address (say a LAN address), the built-in GUI cannot reach the server; loopback or the default work. Left as is: a user who binds elsewhere runs headless or uses the browser.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
