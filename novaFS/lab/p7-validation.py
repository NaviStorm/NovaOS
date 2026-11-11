#!/usr/bin/env python3
"""Reproduire en série les échanges P7 et le banc de latence sur images jetables Darwin."""
import argparse
from collections import defaultdict
import gzip
import hashlib
import json
from math import ceil
import os
from pathlib import Path
import platform
import runpy
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
ARCHES = ("aarch64", "x86_64")
CPUS = {"aarch64": 0x0100000C, "x86_64": 0x01000007}


def build(arch):
    env = dict(os.environ, NOVAFS_TARGET=arch + "-apple-darwin")
    out = subprocess.check_output([ROOT / "lab/run", "build", "--offline", "--example", "p7-lab",
                                   "--message-format=json"], env=env, text=True)
    binary, = (Path(m["executable"]) for line in out.splitlines() if (m := json.loads(line)).get("executable")
               and m["target"]["name"] == "p7-lab")
    with binary.open("rb") as f:
        assert f.read(8) == bytes.fromhex("cffaedfe") + CPUS[arch].to_bytes(4, "little")
    return binary


def run(binary, *args):
    out = subprocess.check_output([binary, *map(str, args)], text=True)
    return [json.loads(line) for line in out.splitlines()]


def run_measured(binary, *args):
    # wait4 rend le RSS propre au processus fils sur Darwin, y compris le
    # runtime de traduction et les bibliothèques. Le flux ne contient que des mesures.
    command = [binary, *map(str, args)]
    with tempfile.TemporaryFile(mode="w+") as output, subprocess.Popen(command, stdout=output) as p:
        _, status, usage = os.wait4(p.pid, 0)
        p.returncode = os.waitstatus_to_exitcode(status)
        if p.returncode:
            raise subprocess.CalledProcessError(p.returncode, command)
        output.seek(0)
        result = [json.loads(line) for line in output]
    assert result[0] == {"repere":"debut"} and result[-1]["repere"] == "fin"
    assert usage.ru_maxrss > 0 and result[-1]["tas_avant"] == result[-1]["tas_apres"]
    return result[1:-1], dict(rss_max_processus_octets=usage.ru_maxrss, **result[-1])


def interop(binaries, legacy):
    results = []
    for producer, consumer in [ARCHES, ARCHES[::-1]]:
        for source in [512, 4096]:
            with tempfile.TemporaryDirectory(prefix="novafs-p7-interop-") as tmp:
                d = Path(tmp)
                run(binaries[producer], "produire", source, d)
                with (d / "pool").open("rb") as f:
                    pool_hash = hashlib.file_digest(f, "sha256").hexdigest()
                # L'ancien écrivain reçoit l'image sur stdin et doit échouer avant
                # de produire le moindre octet d'une nouvelle image.
                with (d / "pool").open("rb") as image:
                    old = subprocess.run([legacy[consumer], "advance", "4096", str(source)],
                                         stdin=image, capture_output=True)
                assert old.returncode == 1 and old.stdout == b"", (old.returncode, old.stderr)
                with (d / "pool").open("rb") as image:
                    assert hashlib.file_digest(image,"sha256").hexdigest() == pool_hash
                source_domain = run(binaries[consumer], "lire", source, d)[0]["domaine_source"]
                (d / "pool").unlink()
                archive_hash = hashlib.sha256((d / "archive").read_bytes()).hexdigest()
                restored = []
                for destination in (512, 4096):
                    result, = run(binaries[consumer], "restaurer", destination, d)
                    domain = result["domaine_restaure"]
                    assert domain != source_domain and domain not in restored
                    restored.append(domain)
                results.append(dict(producteur=producer, lecteur=consumer, secteur=source, restaurations=2,
                                    pool_sha256=pool_hash, archive_sha256=archive_hash, domaines_distincts=3,
                                    refus_ancien_ecrivain="ea00a755", sortie_ancien_ecrivain_octets=0))
    return results


def summarize(samples):
    ns = sorted(v["mur_ns"] for v in samples)
    n = len(ns)
    secs = sum(ns) / 1e9
    amount = sum(v["octets"] for v in samples)
    writes = sum(v["ecritures_blocs"] for v in samples)
    return dict(n=n, p50_ms=ns[ceil(.5*n)-1]/1e6, p95_ms=ns[ceil(.95*n)-1]/1e6,
                p99_ms=ns[ceil(.99*n)-1]/1e6, max_ms=ns[-1]/1e6, cpu_ms=sum(v["cpu_ns"] for v in samples)/1e6,
                debit_mio_s=amount/2**20/secs if amount and secs else None,
                ecriture_physique_sur_logique=writes*4096/amount if amount and writes else None,
                memoire_max_octets=max(v["tas_octets"]+v["pile_reservee_octets"] for v in samples),
                flushs=sum(v["flushs"] for v in samples), flush_ms=sum(v["flush_ns"] for v in samples)/1e6)


def measure(binaries, target, repetitions):
    clocks = {}
    processes = []
    groups = defaultdict(list)
    with gzip.open(target / "p7-mesures.jsonl.gz", "wt", encoding="utf-8") as out:
        def record(arch, fmt, codec, pattern, sector, rep, events):
            for event in events:
                op = event["op"]
                groups[arch, fmt, codec, pattern, op].append(event)
                out.write(json.dumps(dict(type="appel", arch=arch, format=fmt, codec=codec, profil=pattern,
                                          secteur=sector, repetition=rep, **event), separators=(",", ":"))+"\n")

        for arch, binary in binaries.items():
            clocks[arch], = run(binary, "horloge")
            record(arch, "aead", "-", "-", 0, 0, run(binary, "crypto"))
            for rep in range(repetitions):
                # L'ordre des profils est inversé à la seconde répétition.
                formats = ("p6","p7-clair","p7-chiffre")
                for fmt in formats if rep % 2 == 0 else formats[::-1]:
                    for sector in (512,4096):
                        for codec in ("none","lz4"):
                            for pattern in ("repetitif","aleatoire"):
                                events, counters = run_measured(binary, "mesure", sector, fmt, codec, pattern, 0x1357 + rep)
                                processes.append(dict(arch=arch, format=fmt, codec=codec, profil=pattern,
                                                      secteur=sector, repetition=rep, **counters))
                                assert sum(e["op"] == "ecriture_clone" for e in events) == 32
                                assert sum(e["op"] == "lecture_clone" for e in events) == 32
                                for e in events:
                                    if e["op"] in ["ecriture_clone","ecriture_initiale","clone","snapshot"]:
                                        assert e["flushs"] == 2, e
                                    if e["op"] in ["lecture_clone","verification","montage","export"]:
                                        assert e["flushs"] == 0, e
                                record(arch,fmt,codec,pattern,sector,rep,events)
                                out.write(json.dumps(dict(type="processus", **processes[-1]), separators=(",",":"))+"\n")
                print(f"Campagne {rep+1}/{repetitions} terminée pour {arch}",flush=True)
        metadata = dict(hote=platform.platform(), repetitions=repetitions, horloges=clocks,
                        executables_sha256={a:hashlib.sha256(b.read_bytes()).hexdigest() for a,b in binaries.items()})
        out.write(json.dumps(dict(type="environnement", **metadata), separators=(",",":"))+"\n")
    results = [dict(arch=a, format=f, codec=c, profil=p, op=o, **summarize(samples))
               for (a,f,c,p,o),samples in groups.items()]
    write(target/"p7-distributions.json", dict(**metadata, processus=processes, groupes=results))


def write(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, separators=(",",":"))+"\n")


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action",choices=["interop","mesures","tout"])
    parser.add_argument("--sortie",required=True,type=Path)
    parser.add_argument("--repetitions",type=int,default=2)
    args=parser.parse_args()
    if args.repetitions < 1:
        parser.error("au moins une répétition est nécessaire")
    args.sortie.mkdir(parents=True,exist_ok=True)
    # Les deux Mach-O sont figés dans le répertoire jetable pour la campagne entière.
    with tempfile.TemporaryDirectory(prefix="novafs-p7-mach-o-") as tmp:
        binaries = {arch:Path(shutil.copy2(build(arch), Path(tmp) / arch)) for arch in ARCHES}
        if args.action != "mesures":
            source = runpy.run_path(str(ROOT/"lab/p51-validation.py"))["extraire"]("ea00a755", Path(tmp))
            builder = runpy.run_path(str(ROOT/"lab/test-pool-interop.py"))["build"]
            legacy = {a:builder(source,a,Path(tmp)/"binaries-p6",["build","--bin","novafs"],"novafs") for a in ARCHES}
            write(args.sortie/"p7-echanges.json",interop(binaries, legacy))
        if args.action != "interop":
            measure(binaries,args.sortie,args.repetitions)


if __name__ == "__main__":
    main()
