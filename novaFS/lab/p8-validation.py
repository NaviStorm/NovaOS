#!/usr/bin/env python3
"""Qualifier la CLI de scrub et mesurer le parcours sur les deux exécutables Darwin."""
import argparse
from contextlib import contextmanager
import gzip
import hashlib
import json
import os
from math import ceil
from pathlib import Path
import platform
import runpy
import subprocess
import tempfile

LAB = Path(__file__).resolve().parent
ROOT = LAB.parent
P7 = runpy.run_path(str(LAB / "p7-validation.py"))
PIPE = runpy.run_path(str(LAB / "p7-cli.py"))["pipe"]
BUILD = runpy.run_path(str(LAB / "test-pool-interop.py"))["build"]
P0 = runpy.run_path(str(LAB / "test-interop.py"))
BASE = "a33cc66f54ef779b9f67e5a940cd9437ef3f59d4"


@contextmanager
def producers(action, readers):
    if action == "mesures":
        yield readers
        return
    with tempfile.TemporaryDirectory(prefix="novafs-p8-compatibilite-") as tmp:
        work = Path(tmp)
        archive = runpy.run_path(str(LAB / "p51-validation.py"))["extraire"](BASE, work)
        yield {arch: BUILD(archive, arch, work / "cibles", ["build", "--example", "p7-lab"], "p7-lab")
               for arch in readers}



def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def command(binary, image, sector, flags, expected, frame=b"", output=None):
    before = digest(image)
    with PIPE(frame) as fd, image.open("rb") as source:
        with tempfile.TemporaryFile() as sink:
            args = ["scrub", str(image.stat().st_size // 4096), str(sector), "--json", *flags]
            p = subprocess.run(["/bin/sh", "-c", f'exec 3<&{fd}; exec "$@"', "p8", binary, *args],
                               pass_fds=(fd,), stdin=source, stdout=sink, stderr=subprocess.PIPE)
            assert p.returncode == expected, (args, p.returncode, p.stderr)
            sink.seek(0)
            if "--repair" in flags:
                data = sink.read()
                if output:
                    assert expected == 4 and len(data) == image.stat().st_size
                    output.write_bytes(data)
                else:
                    assert not data
                report = json.loads(p.stderr)
            else:
                assert not p.stderr
                report = json.load(sink)
    assert digest(image) == before and report["schema"] == "novafs-scrub-1"
    assert len(report["execution"]) == 32
    assert report["constats_corruption"] == sum(issue["corruption"] for issue in report["anomalies"])
    assert report["lectures_blocs"] >= report["blocs_distincts_examines"] > 0
    return report


def cli(binary, reader, historical_reader, arch, directory, sector):
    assert b"novafs scrub" in subprocess.check_output([binary, "--help"])
    assert b"--repair" in subprocess.check_output([binary, "scrub", "--help"])
    image = directory / "pool"
    good = digest(image)
    with image.open("rb") as source:
        text = subprocess.run([binary, "scrub", "4096", str(sector)], stdin=source,
                              capture_output=True, text=True)
        assert text.returncode == 3 and "tags et contenu chiffré non vérifiés" in text.stdout
    with image.open("rb") as source, tempfile.TemporaryFile() as aliased:
        p = subprocess.run([binary, "scrub", "4096", str(sector), "--repair", "--json"],
                           stdin=source, stdout=aliased, stderr=aliased)
        assert p.returncode == 2 and aliased.tell() == 0
    with image.open("rb") as source:
        p = subprocess.run([binary, "scrub", "4096", str(sector), "--full", "--full", "--json"],
                           stdin=source, capture_output=True)
        assert p.returncode == 2 and json.loads(p.stdout)["statut"] == "usage"
    domain = bytes(P7["run"](reader, "lire", sector, directory)[0]["domaine_source"])
    frame = b"NP7KEYS1" + bytes([38])*16 + bytes([11])*32 + bytes([1]) + domain + bytes([2,32,0]) + bytes([23])*32
    keyed = ["--full", "--cles-fd3"]
    reports = []
    def check(flags, code, **kwargs):
        report = command(binary, image, sector, flags, code, frame, **kwargs)
        reports.append(report)
        return report
    assert check([], 3)["vues_chiffrees_non_verifiees"]
    assert not check(["--full"], 7)["authenticite_verifiee"]
    head = check(keyed, 0)
    assert head["authenticite_verifiee"] and not head["anomalies"]
    offset = 4096*(2+2*(head["generation"]%4))+4095
    with image.open("r+b") as f:
        f.seek(offset)
        v = f.read(1)[0]
        f.seek(offset)
        f.write(bytes([v^1]))
    assert check(keyed, 1)["reparations_possibles"] == 1
    restored = directory/"verification"
    fixed = check(["--repair","--cles-fd3"], 4, output=restored)
    assert (fixed["reparations_effectuees"], fixed["ecritures_blocs"], fixed["synchronisations"]) == (1,1,2)
    assert digest(restored) == good
    restored.replace(image)
    P7["run"](reader, "lire", sector, directory)
    P7["run"](historical_reader, "lire", sector, directory)
    with image.open("r+b") as f:
        f.seek(4095)
        f.write(b"\x81")
    assert check(["--repair","--cles-fd3"], 5)["defauts_non_reparables"] > 0

    target = arch+"-apple-darwin"
    image.write_bytes(P0["extract"](P0["run"](P0["executable"](target,LAB/"run"), "export", sector)))
    assert digest(image) == P0["P0_SHA256"]
    assert check([],0)["format"] == "LV P0"
    writer = BUILD(LAB/"run", arch, ROOT/"target", ["test","--no-run","--test","volumes"], "volumes")
    image.unlink()
    subprocess.run([writer,"p2_pool_image_de_mesure","--exact","--ignored"],
                   env=dict(os.environ,NOVAFS_POOL_FIXTURE=str(image)),stdout=subprocess.PIPE,check=True)
    legacy = check([],0)
    assert legacy["format"] == "catalogue P0" and legacy["objets_examines"] == 16
    data = b"temoin physique P8 sur volume partage"
    for version in ("p5","p6"):
        with image.open("wb") as f:
            subprocess.run([binary,"pool-format","2048",str(sector),"45"*16,version],
                           input=b"",stdout=f,stderr=subprocess.PIPE,check=True)
        for name,tail in [
                ("volume-create",["55"*16,"65"*16,"75"*16,"256"]),
                ("file-create",["1","/preuve"]),
                ("file-write",["1","/preuve","0",data.hex()]),
                ("snapshot",["1"])]:
            with image.open("rb") as src, restored.open("wb") as dst:
                subprocess.run([binary,name,"2048",str(sector),*tail],stdin=src,stdout=dst,
                               stderr=subprocess.PIPE,check=True)
            restored.replace(image)
        assert check(["--full"],0)["format"] == version.upper()
        raw=image.read_bytes()
        assert raw.count(data)==1
        pos=raw.index(data)
        with image.open("r+b") as f:
            f.seek(pos)
            f.write(bytes([data[0]^1]))
        broken=check(["--repair"],5)
        assert {i["objet"] for i in broken["anomalies"] if i["plage_blocs"]==[pos//4096,1]} == {1,2}
    return reports


def stats(rows, process):
    for field in ("lectures", "distincts", "ecritures", "flushs", "pile"):
        assert all(row[field] == rows[0][field] for row in rows), field
    times = sorted(r["mur_ns"] for r in rows)
    def pct(f): return round(times[ceil(len(times)*f)-1] / 1e6, 3)
    seconds = sum(times)/1e9
    return dict(n=len(rows),p50_ms=pct(.5),p95_ms=pct(.95),p99_ms=pct(.99),max_ms=pct(1),
                cpu_moyen_ms=round(sum(r["cpu_ns"] for r in rows)/len(rows)/1e6,3),
                lectures_par_controle=rows[0]["lectures"],blocs_distincts=rows[0]["distincts"],
                debit_lu_Mio_s=round(sum(r["lectures"] for r in rows)/256/seconds,2),
                debit_distinct_Mio_s=round(sum(r["distincts"] for r in rows)/256/seconds,2) if rows[0]["distincts"] else None,
                tas_pic_octets=max(r["tas_pic"] for r in rows),pile_octets=rows[0]["pile"],
                allocations_max=max(r["allocations"] for r in rows),ecritures=rows[0]["ecritures"],
                synchronisations=rows[0]["flushs"],**process)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("cli", "mesures"))
    parser.add_argument("--sortie", type=Path, required=True)
    parser.add_argument("--repetitions", type=int, default=100)
    args = parser.parse_args()
    if args.repetitions < 1:
        parser.error("répétitions positives requises")
    architectures = P7["ARCHES"]
    readers = {arch: P7["build"](arch) for arch in architectures}
    kind, name = ("--bin", "novafs") if args.action == "cli" else ("--example", "p8-mesures")
    binaries = {
        arch: BUILD(LAB / "run", arch, ROOT / "target", ["build", kind, name], name)
        for arch in architectures
    }
    results, raw, clocks = [], [], {}
    if args.action == "mesures":
        clocks = {arch: P7["run"](readers[arch], "horloge")[0] for arch in architectures}
    with producers(args.action, readers) as sources:
        for sector in (512, 4096):
            with tempfile.TemporaryDirectory(prefix="novafs-p8-") as tmp:
                directory = Path(tmp)
                if args.action == "mesures":
                    # Les deux architectures mesurent exactement les mêmes octets.
                    P7["run"](readers[architectures[0]], "produire", sector, directory)
                    image_hash = digest(directory / "pool")
                for arch in architectures:
                    if args.action == "cli":
                        with tempfile.TemporaryDirectory(dir=tmp) as child:
                            work = Path(child)
                            producer = next(t for t in architectures if t != arch)
                            P7["run"](sources[producer], "produire", sector, work)
                            reports = cli(binaries[arch], readers[arch], sources[producer], arch, work, sector)
                            results.append(dict(architecture=arch, producteur=producer,
                                                revision_source=BASE, secteur=sector, rapports=reports))
                    else:
                        for operation in ("controle_p72", "diagnostic", "complet", "reparation"):
                            rows, process = P7["run_measured"](
                                binaries[arch], directory / "pool", sector, args.repetitions, operation)
                            context = dict(architecture=arch, secteur=sector, operation=operation)
                            results.append(dict(**context, image_sha256=image_hash, **stats(rows, process)))
                            raw.extend(dict(**context, **r) for r in rows)
    args.sortie.parent.mkdir(parents=True, exist_ok=True)
    report = dict(macos=platform.mac_ver()[0], horloges=clocks, resultats=results,
                  executables_sha256={arch: digest(path) for arch, path in binaries.items()})
    args.sortie.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    if raw:
        lines = "\n".join(json.dumps(row, separators=(",", ":")) for row in raw) + "\n"
        args.sortie.with_suffix(".jsonl.gz").write_bytes(gzip.compress(lines.encode(), mtime=0))


if __name__ == "__main__":
    main()
