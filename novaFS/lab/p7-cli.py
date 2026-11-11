#!/usr/bin/env python3
"""Parcours de la CLI P7 sur images et secrets jetables. Aucun secret sur disque."""
import argparse
import contextlib
import fcntl
import json
import os
from pathlib import Path
import secrets
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]


@contextlib.contextmanager
def pipe(value):
    read, write = os.pipe()
    try:
        # Le lanceur réserve 3 et 4 dans l'enfant, quelle que soit la numérotation du parent.
        high = fcntl.fcntl(read, fcntl.F_DUPFD, 10)
    finally:
        os.close(read)
    try:
        with os.fdopen(write, "wb") as stream:
            stream.write(value)
        yield high
    finally:
        os.close(high)


def scenario(binary, sector):
    pool, dest, owner, fs, lv = [secrets.token_bytes(16) for _ in range(5)]
    admin, recovery, backup, new_recovery = [secrets.token_bytes(32) for _ in range(4)]
    phrase = secrets.token_bytes(40)
    data = b"Donnees confidentielles de la fixture CLI" * 16

    def frame(identity, domain=None, key=None):
        bundle = b"NP7KEYS1" + identity + admin
        if domain is None:
            return bundle + bytes([0])
        return bundle + bytes([1]) + bytes.fromhex(domain) + bytes([2, 32, 0]) + key

    def enroll(key):
        return len(phrase).to_bytes(2, "little") + phrase + key

    with tempfile.TemporaryDirectory(prefix="novafs-p7-cli-") as tmp:
        directory = Path(tmp)
        empty = directory / "vide"
        empty.touch()

        def run(args, src, dst, auth, payload=b"", expected=0):
            # Le script ne contient que deux numéros de descripteurs contrôlés ici.
            with pipe(auth) as first, pipe(payload) as second, src.open("rb") as source, dst.open("wb") as output:
                script = f'exec 3<&{first} 4<&{second}; exec "$@"'
                r = subprocess.run(["/bin/sh", "-c", script, "p7", binary, *args],
                                   pass_fds=(first, second), stdin=source, stdout=output,
                                   stderr=subprocess.PIPE)
            assert r.returncode == expected, (args[0], r.returncode, r.stderr.decode())
            return r.stderr

        def action(command, tail, src, dst, auth, extra=b""):
            return run([command, "1800", str(sector), *tail, "--cles-fd3"], src, dst, auth, extra)

        a, b, archive, report = (directory / name for name in ("a", "b", "archive", "rapport"))
        base = frame(pool)
        action("pool-format", [pool.hex(), "p7"], empty, a, base)
        info = json.loads(action("volume-create", [owner.hex(), fs.hex(), lv.hex(), "256", "lz4", "chiffre"],
                                 a, b, base, enroll(recovery)))
        opened = frame(pool, info["domaine"], recovery)
        action("file-create", ["1", "/preuve"], b, a, opened)
        action("file-write", ["1", "/preuve", "0", "--donnees-fd4"], a, b, opened, data)
        assert data not in b.read_bytes()
        snap = json.loads(action("snapshot", ["1"], b, a, opened))
        action("export", [str(snap["objet"])], a, archive, opened, backup)
        assert data not in archive.read_bytes()
        a.unlink()
        b.unlink()
        run(["check-export", "--cle-fd4"], archive, report, b"", backup)
        assert json.loads(report.read_bytes())["authenticite_verifiee"]
        target = json.loads(action("restore", [dest.hex(), owner.hex(), "none"], archive, a,
                                   frame(dest), backup + enroll(new_recovery)))
        assert target["domaine"] != info["domaine"]
        new_frame = frame(dest, target["domaine"], new_recovery)
        action("file-read", [str(target["objet"]), "/preuve", "0", "4096"], a, b, new_frame)
        assert b.read_bytes() == data
        action("check", [dest.hex(), str(target["objet"]), target["empreinte"]], a, report, new_frame)
        assert json.loads(report.read_bytes())["recu_verifie"]
        print(binary.parent.parent.name, sector, "création, snapshot, export et restauration validés")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("architecture", choices=["aarch64", "x86_64"])
    args = parser.parse_args()
    target = args.architecture + "-apple-darwin"
    subprocess.run([ROOT / "lab/run", "build", "--offline", "--bin", "novafs"],
                   env=dict(os.environ, NOVAFS_TARGET=target), check=True)
    for sector in [512, 4096]:
        scenario(ROOT / "target" / target / "release" / "novafs", sector)
