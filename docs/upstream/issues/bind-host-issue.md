Title: Server: choose the bind address (--host / STROM_HOST)

## What is missing

The server always binds `0.0.0.0`, in native GUI mode and headless, with no way to restrict it. A desktop setup where only a GUI on the same machine talks to Strom wants the API reachable on loopback only, not on every interface.

## Proposed direction

- `server.host` in the config file and `STROM_SERVER_HOST`, next to `server.port` / `STROM_SERVER_PORT`.
- `--host` / `STROM_HOST` on the command line, which wins over the config file, as for the port.
- Both bind sites use `SocketAddr::new(config.host, config.port)`. The value is an IP address (`IpAddr`): `127.0.0.1`, `::1`, `0.0.0.0`; a host name such as `localhost` is refused when parsing. Whether `::` binds dual-stack depends on the system (`bindv6only`).
- Default unchanged (`0.0.0.0`), so Docker and network access keep working.

Scope: this covers the HTTP/WebSocket listener only. The discovery RTSP server still binds `0.0.0.0:RTSP_PORT` (`discovery/mod.rs`, `start_rtsp_server`), mDNS announces on all interfaces, and blocks open their own media ports. The native GUI connects to `localhost:{port}`, so with a non-loopback address it cannot reach the server.

## Questions for the maintainers

1. Should the same setting also restrict the RTSP server and mDNS, or should those get their own settings?
2. Is `0.0.0.0` still the right default, or should the native GUI mode default to loopback?
3. Accept host names (resolved at startup) or only IP addresses?

## Test case

Config tests: the default is `0.0.0.0`; the config file reads `host = "127.0.0.1"`; `STROM_SERVER_HOST=::1` is read from the environment. Manual: `strom --headless --host 127.0.0.1 --port 18765`; `ss -ltn` shows only `127.0.0.1:18765` for the API, and `GET /api/flows` on 127.0.0.1 answers 200.

A branch with this exists on top of current `main` and can become a PR once the direction is agreed.
