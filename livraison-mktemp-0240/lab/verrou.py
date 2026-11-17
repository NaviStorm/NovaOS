"""Garantir que la campagne invitée détient le verrou ARC-027 partagé."""
import fcntl
import os
from pathlib import Path
import subprocess
import sys

with (Path(os.environ["NOVAOS_TMPDIR"])/"arc027-mesures.lock").open("a") as lock:
    fcntl.flock(lock, fcntl.LOCK_EX)
    result = subprocess.run(sys.argv[1:], env=dict(os.environ, MKTEMP_MESURE_VERROUILLEE="1"))
    sys.exit(result.returncode)
