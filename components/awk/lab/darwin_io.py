"""Contrôles macOS du processus réel, de ses pipes et des limites de ressources."""
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time

exe = str(Path(sys.argv[1]).resolve(strict=True))
report = Path(sys.argv[2]).resolve()
env = dict(os.environ, LC_ALL="C")
results = {}


def record(name, process, stdout, stderr, ok):
    results[name] = dict(status=process.returncode, stdout=stdout.hex(), stderr=stderr.hex(), ok=bool(ok))


with tempfile.TemporaryDirectory(dir=env["NOVAOS_TMPDIR"]) as tmp:
    cwd = Path(tmp)
    source = cwd / "entree"
    source.write_bytes(b"alpha 1\nbeta 2\ngamma 3")
    with subprocess.Popen(["/bin/cat", source], stdout=subprocess.PIPE) as cat:
        p = subprocess.run([exe, "{print $1}"], stdin=cat.stdout, capture_output=True, env=env)
        cat.stdout.close()
        record("cat-pipe-awk", p, p.stdout, p.stderr,
               cat.wait(timeout=5) == 0 and p.returncode == 0 and p.stdout == b"alpha\nbeta\ngamma\n" and not p.stderr)
    r, w = os.pipe()
    os.close(r)
    try:
        p = subprocess.run([exe, "BEGIN{print 1}"], stdout=w, stderr=subprocess.PIPE, env=env)
        record("sortie-fermee", p, b"", p.stderr, p.returncode != 0 and p.stderr)
    finally:
        os.close(w)
    proc = subprocess.Popen([exe, "{print}"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
    try:
        time.sleep(0.08)
        proc.send_signal(signal.SIGINT)
        out, err = proc.communicate(timeout=5)
        record("interruption", proc, out, err, proc.returncode == -signal.SIGINT)
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()
    for name, args, expected in [
            ("entree-repertoire", ["{print}", tmp], b""),
            ("epuisement-table-champs", ["BEGIN{NF=1e30; print 7}"], b"out of memory"),
            ("fichier-absent", ["{print}", str(cwd/"absent")], b""),
            ("champ-negatif", ["BEGIN{NF=-1}"], b"negative NF")]:
        p = subprocess.run([exe, *args], env=env, capture_output=True, timeout=5)
        record(name, p, p.stdout, p.stderr, p.returncode == 2 and not p.stdout and p.stderr and expected in p.stderr)
    for locale, marker in [("fr-CA", b"motif"), ("en", b"Usage"), ("de", b"Usage")]:
        p = subprocess.run([exe, "-f", "absent", "--help"], cwd=cwd,
                           env=dict(env, LC_ALL=locale), capture_output=True)
        record("aide-"+locale, p, p.stdout, p.stderr, p.returncode == 0 and marker.lower() in p.stdout.lower() and not p.stderr)
    if list(cwd.iterdir()) != [source]:
        raise RuntimeError("effet de bord du contrôle documentaire")
report.write_text(json.dumps(dict(sha256=hashlib.sha256(Path(exe).read_bytes()).hexdigest(), tests=results), indent=1)+"\n")
print({k:r["ok"] for k,r in results.items()})
sys.exit(int(not all(v["ok"] for v in results.values())))
