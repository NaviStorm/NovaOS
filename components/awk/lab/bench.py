"""Banc Darwin ARM64. Un sous-processus, un fichier et un relevé wait4 par essai."""
import argparse
import hashlib
import json
import os
import platform
import resource
import shutil
import statistics
import subprocess
import tempfile
import time
from pathlib import Path


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def corpus(path, count, unique=False):
    # Graine 0. Un cycle de 1024 enregistrements, chacun de 128 octets.
    def line(i):
        key = i if unique else i % 16
        tag = "erreur" if i % 1024 == 0 else "info"
        return f"k{key:08d} 2 {tag}:x:3:".encode().ljust(127, b".") + b"\n"

    with path.open("wb") as out:
        block = b"".join(line(i) for i in range(1024))
        for start in range(0, count, 1024):
            n = min(count - start, 1024)
            out.write(b"".join(line(i) for i in range(start, start+n))
                      if unique else block[:128*n])
        if count * 128 >= 2**32:
            out.write(b"TEMOIN-4G\n")


def workloads(n, unique=False):
    if unique:
        yield "cles-croissantes", [], "{s[$1]+=$2} END {for(k in s) print k,s[k]}", "map"
        return
    marker = n*128 >= 2**32
    yield "comptage", [], "END {print NR}", str(n + marker)
    if marker:
        yield "temoin", [], '/^TEMOIN-4G$/ {print NR,$0} END {print NR}', f"{n+1} TEMOIN-4G\n{n+1}"
        return
    yield "somme", ["-F:"], "{s+=$3} END {print s}", str(3*n)
    yield "rare", [], "/erreur/ {n++} END {print n+0}", str((n+1023)//1024)
    yield "frequent", [], "/info/ {n++} END {print n+0}", str(n-(n+1023)//1024)
    yield "cles-fixes", [], "{s[$1]+=$2} END {for(k in s) print k,s[k]}", "map"
    yield "sortie", [], "{print $1}", "keys"


def correct(path, expected, n, unique):
    if expected == "keys":
        h = hashlib.sha256()
        block = b"".join(f"k{i:08d}\n".encode() for i in range(16))
        for _ in range(n//1024):
            h.update(block*64)
        h.update((block*64)[:(n%1024)*10])
        return h.hexdigest() == digest(path)
    if expected == "map":
        seen = set()
        with path.open("rb") as stream:
            for line in stream:
                cols = line.rstrip(b"\n").split(b" ")
                if len(cols) != 2 or len(cols[0]) != 9 or not cols[0][1:].isdigit():
                    return False
                k = int(cols[0][1:])
                limit = n if unique else min(16, n)
                total = 2 if unique else 2*((n+15-k)//16)
                if k >= limit or k in seen or line != f"k{k:08d} {total}\n".encode():
                    return False
                seen.add(k)
        return len(seen) == (n if unique else min(16,n))
    return path.read_bytes() == expected.encode()+b"\n"


def run(exe, args, path, output, errors):
    with output.open("wb") as out, errors.open("wb") as err:
        start = time.monotonic()
        p = subprocess.Popen([exe, *args, path], stdout=out, stderr=err,
                             stdin=subprocess.DEVNULL, env={**os.environ, "LC_ALL": "C"})
        _, status, usage = os.wait4(p.pid, 0)
        elapsed = time.monotonic()-start
        p.returncode = os.waitstatus_to_exitcode(status)
    return dict(status=p.returncode, seconds=elapsed, user=usage.ru_utime,
                system=usage.ru_stime, rss_bytes=usage.ru_maxrss,
                stderr=errors.read_bytes()[:2048].decode(errors="replace"))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--mib", type=int, required=True)
    ap.add_argument("--unique", action="store_true")
    ap.add_argument("--count-only", action="store_true")
    ap.add_argument("--results", required=True, type=Path)
    ap.add_argument("binaries", nargs="+")
    args = ap.parse_args()
    if platform.system() != "Darwin" or args.mib <= 0:
        ap.error("Darwin requis, volume strictement positif")
    resource.setrlimit(resource.RLIMIT_CORE,(0,0))
    n = args.mib * 8192
    root = Path(os.environ["NOVAOS_TMPDIR"]).resolve(strict=True)
    with tempfile.TemporaryDirectory(dir=root) as tmp, args.results.open("x") as report:
        path, output, errors = [Path(tmp)/x for x in ("input","stdout","stderr")]
        corpus(path,n,args.unique)
        size = path.stat().st_size
        print(json.dumps(dict(input_sha256=digest(path), bytes=size, records=n+(size>n*128),
                              seed=0, platform=platform.platform(), locale="C",
                              destination="fichier APFS temporaire", unique=args.unique)),file=report)
        for name, opts, prog, expected in workloads(n,args.unique):
            if args.count_only and name not in {"comptage","temoin"}:
                continue
            for binary in args.binaries:
                exe = Path(shutil.which(binary) or binary).resolve(strict=True)
                sha = digest(exe)
                trials = []
                for repeat in range(4):
                    trial = run(exe, [*opts, prog], path, output, errors)
                    ok = trial["status"] == 0 and not trial["stderr"] and correct(output,expected,n,args.unique)
                    trial.update(ok=ok, repeat=repeat, workload=name, executable=str(exe),
                                 sha256=sha, program=prog, options=opts, output_sha256=digest(output),
                                 output_bytes=output.stat().st_size)
                    if ok:
                        trial.update(mib_s=size/2**20/trial["seconds"],
                                     records_s=(n+(size>n*128))/trial["seconds"])
                    print(json.dumps(trial), file=report, flush=True)
                    if not ok:
                        break
                    if repeat:
                        trials.append(trial["mib_s"])
                if len(trials)==3:
                    print(name,exe.name,round(statistics.median(trials),2),
                          [round(min(trials),2),round(max(trials),2)],flush=True)


if __name__ == "__main__":
    main()
