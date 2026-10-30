"""Sums a qmlprofiler trace (.qtd) by event: QML bindings, JavaScript
functions, signal handlers, object creation. Self time (without the
events nested inside) and total time, the busiest first, each as a
share of the recorded seconds.

    python3 -I scripts/qml-profile.py trace.qtd SECONDS [ROWS]
"""

import sys
import xml.etree.ElementTree as ET
from collections import defaultdict

KINDS = {"Binding", "Javascript", "HandlingSignal", "Creating", "Compiling"}


def main():
    path, seconds = sys.argv[1], float(sys.argv[2])
    rows = int(sys.argv[3]) if len(sys.argv) > 3 else 30
    root = ET.parse(path).getroot()
    events = {}
    for ev in root.iter("event"):
        kind = ev.findtext("type") or ""
        if kind not in KINDS:
            continue
        where = ev.findtext("displayname") or "?"
        details = " ".join((ev.findtext("details") or "").split())
        events[ev.get("index")] = (kind, where, details[:70])
    ranges = []
    for r in root.iter("range"):
        index = r.get("eventIndex")
        if index in events and r.get("duration") is not None:
            start = int(r.get("startTime"))
            ranges.append((start, start + int(r.get("duration")), index))
    # Nesting by time: a range inside the one before it is its child.
    ranges.sort(key=lambda r: (r[0], -r[1]))
    self_ns, total_ns, count = defaultdict(int), defaultdict(int), defaultdict(int)
    stack = []  # [end, index, child time]
    top_ns = 0

    def close(entry):
        end, index, child, start = entry
        self_ns[index] += end - start - child

    for start, end, index in ranges:
        while stack and stack[-1][0] <= start:
            close(stack.pop())
        total_ns[index] += end - start
        count[index] += 1
        if stack:
            stack[-1][2] += end - start
        else:
            top_ns += end - start
        stack.append([end, index, 0, start])
    while stack:
        close(stack.pop())

    print(f"QML/JS on the GUI thread: {top_ns / 1e9:.2f} s of {seconds:.0f} s ({top_ns / 1e7 / seconds:.0f} %)")
    by_kind = defaultdict(int)
    for index, ns in self_ns.items():
        by_kind[events[index][0]] += ns
    print("  " + ", ".join(f"{k} {ns / 1e7 / seconds:.1f} %" for k, ns in sorted(by_kind.items(), key=lambda x: -x[1])))
    print(f"\n{'self %':>7} {'total %':>8} {'count':>7}  kind        where / what")
    for index in sorted(self_ns, key=lambda i: -self_ns[i])[:rows]:
        kind, where, details = events[index]
        print(f"{self_ns[index] / 1e7 / seconds:7.2f} {total_ns[index] / 1e7 / seconds:8.2f} {count[index]:7d}  "
              f"{kind:<11} {where}  {details}")


if __name__ == "__main__":
    main()
