"""Mesure Darwin par processus, entrée régulière fictive et cache chaud."""
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys
import tempfile

candidate, report = sys.argv[1:]
oracle = "/opt/homebrew/opt/grep/libexec/gnubin/grep"
env = {**os.environ, "LC_ALL": "C", "LANG": "C"}
version = subprocess.check_output([oracle, "--version"], env=env).decode()
assert version.startswith("grep (GNU grep) 3.12\n")
pcre = subprocess.run([oracle, "-Po", "(?<=a)b"], input=b"ab\n", capture_output=True, env=env)
assert (pcre.returncode, pcre.stdout, pcre.stderr) == (0, b"b\n", b"")
rows = []
with tempfile.TemporaryDirectory(dir=env["TMPDIR"]) as directory:
    for size in (8, 64):
        f = Path(directory)/"journal"
        record = b"x" * 60 + b"fin\n"
        with f.open("wb") as stream:
            for _ in range(size):
                stream.write(record * 16384)
        for pattern, mode in [("fin", "-F"), ("f.n", "-E"), ("(?<=f)in", "-P")]:
            for executable in (oracle, candidate):
                for repetition in range(3):
                    p = subprocess.run(["/usr/bin/time", "-lp", executable, mode, "-c", pattern, str(f)],
                                       capture_output=True, env=env, check=True)
                    assert int(p.stdout) == f.stat().st_size // len(record)
                    usage = p.stderr.decode()
                    rows.append(dict(mio=size, mode=mode, binaire=Path(executable).resolve().name,
                                     candidat=executable == candidate, repetition=repetition,
                                     secondes=float(re.search(r"real\s+([\d.]+)", usage)[1]),
                                     rss_octets=int(re.search(r"(\d+)\s+maximum resident set size", usage)[1]),
                                     brut=usage))
Path(report).write_text(json.dumps(dict(locale="C", oracle=version, pcre2=True,
        arch=platform.machine(), os=platform.mac_ver()[0], mesures=rows), indent=1)+"\n")
