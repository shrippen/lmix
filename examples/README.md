# Example clients

Small programs that control lmix or follow its tally (API.11), as a start
for your own. They use the text protocol (`lmix-core --tcp <address>`,
described in `workspace/lmix-core/src/tcp.rs`); the HTTP API and the
event stream are described in `workspace/lmix-core/src/api.rs`, the
actions with their groups at `GET /api/actions`.

| Folder | What |
|---|---|
| `python/lmix_client.py` | Run an action, read or follow the tally; standard library only |
| `tally-micropython/main.py` | Tally light for one camera on an ESP32 (MicroPython): red on program, green in preview |

Without programming: `lmix-core --tally-http <address>` calls a tally
light over HTTP itself (POST JSON, or GET with `{program}`, `{preview}`).
