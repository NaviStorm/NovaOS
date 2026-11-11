"""Transactions du moteur de NovaFS, sur un LV autonome produit par la CLI macOS."""
import argparse
import hashlib
import os
from pathlib import Path
import re
import secrets
import subprocess
import tempfile
from bloc import Bench, ROOT

BLOCKS = 256
HOST = ROOT.parent / "novaFS/target/aarch64-apple-darwin/release/novafs"
SOURCE = b"hote-Nova!"
PATTERN = bytes(((i * 73) & 255) ^ (i >> 5) for i in range(4231))
EXPECTED = PATTERN[:4091] + b"NovaFS-partiel!" + PATTERN[4106:]


def oracle(bench, expected):
    def cli(*args):
        with bench.image.open("rb") as image:
            return subprocess.run([HOST, args[0], str(bench.image.stat().st_size // 4096), str(bench.sector), *args[1:]],
                                  stdin = image, stdout=subprocess.PIPE, check=True).stdout
    heads = re.findall(r"slot=(\d+) génération=(\d+) blocs=\d+ FS=(\w+) LV=(\w+)".encode(), cli("inspect"))
    assert len(heads) == 2
    slot, gen, fs, lv = max(heads, key=lambda h: int(h[1]))
    cli("verify", fs, lv)
    assert cli("salvage", fs, lv, slot, "/source") == SOURCE
    if expected is not None:
        assert cli("salvage", fs, lv, slot, "/sortie") == expected
    return int(gen)


def fixture(bench, existing=False, blocks=BLOCKS):
    with bench.image.open("wb") as image:
        commands = b"create /source\nwrite /source 0 " + SOURCE.hex().encode() + b"\n"
        if existing:
            commands += b"create /sortie\nwrite /sortie 0 616e6369656e\n"
        subprocess.run([HOST, "script", str(blocks), str(bench.sector)],
                       input = commands, stdout=image, stderr=subprocess.PIPE, check=True)
    return oracle(bench, b"ancien" if existing else None)


def normal(bench):
    before = fixture(bench)
    s = bench.run("files-write")
    assert "FICHIERS CLOSE" in s and "BLOC session terminee ok=true" in s, s[ - 1400:]
    assert oracle(bench, EXPECTED) == before + 3
    previous = bench.image.read_bytes()
    s = bench.run("files-read", readonly=True)
    assert "FICHIERS CLOSE" in s and "BLOC session terminee ok=true" in s, s[ - 1400:]
    assert bench.image.read_bytes() == previous
    bench.check("fichier hôte, création et deux transactions, oracle hôte et nouvelle VM RO")


CUTS = [
    ("files-data", "COUPURE donnees", "ancien"),
    ("files-meta", "COUPURE metadonnees", "ancien"),
    ("files-f1-before", "COUPURE avant F1", "ancien"),
    ("files-f1-inflight", "COUPURE armer blkdebug", "ancien"),
    ("files-f1-after", "COUPURE apres F1", "ancien"),
    ("files-f2-before", "COUPURE avant F2", "indetermine"),
    ("files-f2-inflight", "COUPURE armer blkdebug", "indetermine"),
    ("files-f2-after", "COUPURE apres F2", "nouveau"),
    ("files-close", "COUPURE fermeture", "nouveau"),
    ("files-remount", "COUPURE remontage", "ancien"),
]


def recover(bench, generation, allowed, label):
    got = oracle(bench, None)
    assert got in (generation, generation + 1)
    new = got == generation + 1
    state = "nouveau" if new else "ancien"
    assert allowed in (state, "indetermine"), (allowed, state)
    oracle(bench, PATTERN if new else b"ancien")
    result = bench.run("files-recover", label + "-reprise", readonly=True)
    assert f"FICHIERS oracle {state}" in result and "BLOC session terminee ok=true" in result
    bench.check(f"{label}, generation {got}, oracle {state}, remontage RO")


def cuts(bench):
    initial = fixture(bench, True)
    base = bench.image.read_bytes()
    for case, marker, wanted in CUTS:
        bench.image.write_bytes(base)
        report = bench.run(case, cut=marker, debug="" if "inflight" in case else None)
        if "FICHIERS transaction confirmee" in report:
            assert wanted == "nouveau"
        recover(bench, initial, wanted, case)


def errors(bench):
    initial = fixture(bench, True)
    base = bench.image.read_bytes()
    for label, event, state, want, code in [
            ("lecture", "read_aio", 1, "ancien", 26),
            ("ecriture", "write_aio", 1, "ancien", 26),
            ("f1", "flush_to_disk", 2, "ancien", 26),
            ("f2", "flush_to_disk", 3, "indetermine", 22)]:
        bench.image.write_bytes(base)
        cfg = bench.tmp / "eio.conf"
        seq = ('[set-state]\nevent = "write_aio"\nstate = "1"\nnew_state = "2"\n'
             if state >= 2 else "")
        if state == 3:
            seq += '[set-state]\nevent = "flush_to_disk"\nstate = "2"\nnew_state = "3"\n'
        cfg.write_text(seq + f'[inject-error]\nevent = "{event}"\nstate = "{state}"\nerrno = "5"\n')
        s = bench.run("files-inject", "fichier-eio-" + label, debug=cfg)
        assert f"client=Exit({code})" in s and "BLOC session terminee ok=false" in s and "FICHIERS transaction confirmee" not in s, s[ - 1400:]
        hardware_op = {"lecture": 0, "ecriture": 1, "f1": 2, "f2": 2}[label]
        assert f"BLOC erreur peripherique op={hardware_op} statut=1" in s and "retrait=Ok(())" in s
        recover(bench, initial, want, "eio-" + label)


def admission(bench):
    initial = fixture(bench, True)
    base = bench.image.read_bytes()
    s = bench.run("files-info", readonly=True)
    heads = subprocess.check_output([HOST, "inspect", str(BLOCKS), str(bench.sector)], input=base)
    identity = re.search(rb"LV=([0-9a-f]{32})", heads)[1].decode()
    assert f"Volume {identity}" in s and "BLOC session terminee ok=true" in s
    denied = bench.run("files-write", "fichier-ro", readonly=True)
    assert "service=Exit(5)" in denied and "BLOC session terminee ok=false" in denied
    s = bench.run("files-abandon")
    assert "client=Exit(9)" in s and "retrait=Ok(())" in s and "FICHIERS abandon" in s
    assert base == bench.image.read_bytes()
    recover(bench, initial, "ancien", "abandon-client")
    s = bench.run("files-restart")
    assert "client=Exit(22)" in s and "service=Exit(9)" in s and "FICHIERS oracle ancien" in s and "BLOC session terminee ok=true" in s, s[ - 1500:]
    assert base == bench.image.read_bytes()
    for label, patch, code in [("inconnu", 0, 25), ("ancre", 4096 + 80, 21)]:
        bad = bytearray(base)
        bad[patch] ^= 128
        bench.image.write_bytes(bad)
        s = bench.run("files-write", "fichier-" + label)
        assert f"client=Exit({code})" in s and "BLOC session terminee ok=false" in s, s[ - 900:]
        assert bench.image.read_bytes() == bad
    generation = fixture(bench, True, 145)
    before = bench.image.read_bytes()
    s = bench.run("files-inject", "reserve-maintenance")
    assert "client=Exit(23)" in s and "FICHIERS transaction confirmee" not in s
    assert before == bench.image.read_bytes()
    recover(bench, generation, "ancien", "reserve-preservee")
    bench.check("refus du format, de l’ancre altérée et du support RO, pénurie et relance du service")


def pools(bench):
    """L’image réellement formatée doit rester identique après le refus RW."""
    identity = secrets.token_bytes(16)
    for profile in ("p5", "p6", "p7"):
        cmd = [HOST, "pool-format", "1800", str(bench.sector), identity.hex(), profile]
        with bench.image.open("wb") as output:
            if profile == "p7":
                # La CLI lit les clés sur un tube anonyme au descripteur 3.
                keys = b"NP7KEYS1" + identity + secrets.token_bytes(32) + bytes([0])
                subprocess.run(["/bin/sh", "-c", 'exec 3<&0; exec "$@" </dev/null',
                                "novafs", *cmd, "--cles-fd3"], input=keys, stdout=output, check=True)
            else:
                subprocess.run(cmd, stdin=subprocess.DEVNULL, stdout=output, check=True)
        def digest():
            with bench.image.open("rb") as f:
                return hashlib.file_digest(f, "sha256").digest()
        before = digest()
        result = bench.run("files-write", "refus-pool-" + profile)
        assert "client=Exit(25)" in result and "BLOC session terminee ok=false" in result
        assert digest() == before
        bench.check(f"pool {profile} intact après refus RW")


def perf(bench):
    fixture(bench, True)
    s = bench.run("files-perf")
    assert "STOCKAGE restitution apres deux sessions=true" in s and "BLOC session terminee ok=true" in s, s[ - 1800:]
    assert s.count("ECHANTILLON ech mount") == 2 and s.count("ECHANTILLON bar f2") == 2
    oracle(bench, PATTERN)
    bench.check("mesures fichiers et restitution sur deux sessions")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sector", type=int, choices=(512, 4096))
    parser.add_argument("--only", choices=("normal", "cuts", "errors", "perf", "admission", "pools"))
    args = parser.parse_args()
    for sector in ([args.sector] if args.sector else [512, 4096]):
        with tempfile.TemporaryDirectory(prefix="novafichiers-") as tmp:
            bench = Bench(Path(tmp), os.environ["NOVA_ARCH"], sector)
            for suite in (normal, cuts, errors, admission, pools, perf):
                if args.only in (None, suite.__name__):
                    suite(bench)
