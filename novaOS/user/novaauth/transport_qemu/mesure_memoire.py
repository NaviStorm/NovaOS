"""Allocations Python conservées par un export neuf. Exécution hors invité."""
from pathlib import Path
import inspect
import json
import platform
import socket
import tracemalloc

from adaptateur import PAGE, Pages


def mesurer():
    left, right = socket.socketpair()
    tracemalloc.start()
    pages = Pages(left)
    snapshot = tracemalloc.take_snapshot()
    tracemalloc.stop()
    try:
        here = str(Path(__file__).resolve())
        constructor = inspect.getsourcefile(Pages)
        # Les sockets et les imports sont antérieurs à la mesure.
        allocations = snapshot.filter_traces((
            tracemalloc.Filter(True, here),
            tracemalloc.Filter(True, constructor),
        )).statistics("lineno")
        total = sum(item.size for item in allocations)
        return dict(python=platform.python_version(), architecture=platform.machine(),
                    allocations=[str(item) for item in allocations],
                    total_octets=total, pages_utiles=2 * PAGE,
                    supplement=total - 2 * PAGE, plafond=256,
                    conforme=total <= 2 * PAGE + 256)
    finally:
        pages.fermer()
        right.close()


if __name__ == "__main__":
    mesure = mesurer()
    print(json.dumps(mesure, ensure_ascii=False, indent=2))
    raise SystemExit(0 if mesure["conforme"] else 1)
