"""Sérialise une campagne locale avec les bancs ARC-027 et CUT."""
from contextlib import ExitStack
import fcntl
import os
from pathlib import Path
import subprocess
import sys

with ExitStack() as stack:
    for name in ("arc027-mesures.lock","cut-performances.lock"):
        lock=stack.enter_context((Path(os.environ["NOVAOS_TMPDIR"])/name).open("a"))
        fcntl.flock(lock,fcntl.LOCK_EX)
    raise SystemExit(subprocess.call(sys.argv[1:]))
