#!/usr/bin/env python3
"""Find lmix's controls by their accessible name (AT-SPI), for the tests
on Xvfb: a click by name still hits when a row gains a button, a click
by pixel does not.

    a11y.py find NAME [--role ROLE] [--within NAME] [--nth N] [--wait S]
        Centre "x y" on the screen of the N-th (from 0) showing control
        whose name matches NAME (a Python regex, whole name, case
        ignored), optionally of ROLE (AT-SPI role name, e.g. "push
        button") and inside a control named --within. Exit 1 if none
        shows within --wait seconds.
    a11y.py dump
        Every named, showing control: role, name, x, y, width, height.

Needs the AT-SPI bus (at-spi-bus-launcher, at-spi2-registryd) and lmix
started with QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1; scripts/test-console-qt.sh
sets both up in a D-Bus session of its own.
"""
import argparse
import re
import sys
import time

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi  # noqa: E402


def app():
    desktop = Atspi.get_desktop(0)
    for i in range(desktop.get_child_count()):
        child = desktop.get_child_at_index(i)
        if child is not None and child.get_name() == "lmix":
            return child
    return None


def showing(node):
    states = node.get_state_set()
    return states.contains(Atspi.StateType.SHOWING) and states.contains(Atspi.StateType.VISIBLE)


def controls(node, inside=False, within=None):
    """(node, name, role, extents) of showing controls, in tree order."""
    try:
        if not showing(node):
            return
        name = node.get_name() or ""
        role = node.get_role_name()
        box = node.get_extents(Atspi.CoordType.SCREEN)
    except Exception:
        return
    if within is not None and not inside and within.fullmatch(name):
        inside = True
    if (within is None or inside) and box.width > 0 and box.height > 0:
        yield node, name, role, box
    for i in range(node.get_child_count()):
        child = node.get_child_at_index(i)
        if child is not None:
            yield from controls(child, inside, within)


def find(args):
    pattern = re.compile(args.name, re.IGNORECASE)
    within = re.compile(args.within, re.IGNORECASE) if args.within else None
    deadline = time.monotonic() + args.wait
    while True:
        root = app()
        if root is not None:
            hits = [
                box
                for _, name, role, box in controls(root, within=within)
                if pattern.fullmatch(name) and (args.role is None or role == args.role)
            ]
            if len(hits) > args.nth:
                box = hits[args.nth]
                if args.box is None:
                    print(box.x + box.width // 2, box.y + box.height // 2)
                else:
                    # The box less `--box` of its size on each side, as a crop geometry.
                    dx, dy = int(box.width * args.box), int(box.height * args.box)
                    print(f"{box.width - 2 * dx}x{box.height - 2 * dy}+{box.x + dx}+{box.y + dy}")
                return 0
        if time.monotonic() >= deadline:
            print(f"no showing control named /{args.name}/", file=sys.stderr)
            return 1
        time.sleep(0.3)


def dump(_args):
    root = app()
    if root is None:
        print("lmix not on the accessibility bus", file=sys.stderr)
        return 1
    for _, name, role, box in controls(root):
        if name:
            print(f"{role:16} {name!r:50} {box.x:5} {box.y:4} {box.width:4} {box.height:4}")
    return 0


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)
    f = sub.add_parser("find")
    f.add_argument("name")
    f.add_argument("--role")
    f.add_argument("--within")
    f.add_argument("--nth", type=int, default=0)
    f.add_argument("--wait", type=float, default=5)
    f.add_argument("--box", type=float, metavar="INSET",
                   help="print the box (WxH+X+Y) less this fraction on each side instead of its centre")
    f.set_defaults(run=find)
    sub.add_parser("dump").set_defaults(run=dump)
    args = parser.parse_args()
    return args.run(args)


if __name__ == "__main__":
    sys.exit(main())
