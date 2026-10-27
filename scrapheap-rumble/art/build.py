"""Entry point: ./art/build.sh [asset ...] regenerates assets/models/*.glb.
With no arguments, builds everything. A module name (parts, props, arena,
yard, workshop) builds that whole module."""
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import lib  # noqa: E402

MODULES = {}
for mod in ("parts", "props", "arena", "yard", "workshop"):
    try:
        MODULES[mod] = __import__(mod).ASSETS
    except ModuleNotFoundError as e:
        if e.name != mod:
            raise

ALL = {k: f for assets in MODULES.values() for k, f in assets.items()}


def main():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    names = []
    for a in argv or list(ALL):
        names += list(MODULES[a]) if a in MODULES else [a]
    for n in names:
        if n not in ALL:
            sys.exit(f"unknown asset {n!r}; known: {', '.join(sorted(ALL))}")
        t = time.time()
        lib.reset()
        res = ALL[n]()
        lib.export(n, **res)
        print(f"  {n}: {time.time() - t:.1f}s")


main()
