#!/usr/bin/env python3
"""A small lmix client over the text protocol (API.5, `lmix-core --tcp`),
standard library only: run actions, read or follow the tally.

    lmix_client.py HOST:PORT act '{"action":"cut"}'
    lmix_client.py HOST:PORT tally            # once: 0 off, 1 program, 2 preview
    lmix_client.py HOST:PORT follow           # a line per tally change

The protocol: lines ending in \\n; the server greets with `LMIX 1`; a JSON
action answers `OK` or `ER <error> <message>`; `SUB TALLY` sends
`TALLY <digits>` on every change. See workspace/lmix-core/src/tcp.rs.
"""
import json
import socket
import sys


class Lmix:
    def __init__(self, address, timeout=5.0):
        host, _, port = address.rpartition(":")
        self.sock = socket.create_connection((host or "127.0.0.1", int(port)), timeout=timeout)
        self.lines = self.sock.makefile("r", encoding="utf-8", newline="\n")
        greeting = self.line()
        if not greeting.startswith("LMIX "):
            raise RuntimeError(f"not lmix: {greeting!r}")

    def line(self):
        text = self.lines.readline()
        if not text:
            raise ConnectionError("lmix closed the connection")
        return text.rstrip("\r\n")

    def send(self, text):
        self.sock.sendall((text + "\n").encode("utf-8"))

    def act(self, action):
        """Runs an action (a dict); raises with lmix's reason if refused."""
        self.send(json.dumps(action))
        answer = self.line()
        if answer != "OK":
            raise RuntimeError(answer)

    def tally(self):
        """Per input in bar order: 0 off, 1 program, 2 preview."""
        self.send("TALLY")
        return self.tally_line(self.line())

    def follow(self):
        """Yields the tally on every change, the current one first."""
        self.send("SUB TALLY")
        if self.line() != "OK":
            raise RuntimeError("SUB TALLY refused")
        self.sock.settimeout(None)
        yield self.tally()
        while True:
            line = self.line()
            if line.startswith("TALLY "):
                yield self.tally_line(line)

    @staticmethod
    def tally_line(line):
        if not line.startswith("TALLY"):
            raise RuntimeError(f"unexpected answer: {line!r}")
        return [int(c) for c in line[len("TALLY"):].strip()]


def main(argv):
    if len(argv) < 3 or argv[2] not in ("act", "tally", "follow") or (argv[2] == "act" and len(argv) < 4):
        print(__doc__.strip(), file=sys.stderr)
        return 2
    lmix = Lmix(argv[1])
    if argv[2] == "act":
        lmix.act(json.loads(argv[3]))
        print("OK")
    elif argv[2] == "tally":
        print("".join(map(str, lmix.tally())))
    else:
        for tally in lmix.follow():
            on_air = [i + 1 for i, t in enumerate(tally) if t == 1]
            preview = [i + 1 for i, t in enumerate(tally) if t == 2]
            print(f"program {on_air or '-'}  preview {preview or '-'}", flush=True)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv))
    except (RuntimeError, ConnectionError, OSError) as e:
        print(f"lmix: {e}", file=sys.stderr)
        sys.exit(1)
    except KeyboardInterrupt:
        sys.exit(130)
