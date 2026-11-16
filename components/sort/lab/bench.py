"""Recette macOS. Les validations et la génération ont leur propre chronomètre."""
import argparse
import hashlib
import json
import os
import platform
import shutil
import threading
import statistics
import subprocess
import tempfile
import time
from pathlib import Path
from corpus import generate, WIDTH, RESERVE

ROOT = Path(__file__).resolve().parents[1]
GNU = Path("/opt/homebrew/bin/gsort")
CANDIDATE = ROOT / "artifacts/darwin-arm64/sort"


def validate(stream, count):
    digest = hashlib.sha256()
    previous = None
    seen = 0
    while block := stream.read(WIDTH * 4096):
        digest.update(block)
        if len(block) % WIDTH:
            raise ValueError("sortie tronquée")
        for start in range(0, len(block), WIDTH):
            row = block[start:start+WIDTH]
            if row[-1] != 10 or (previous is not None and row < previous):
                raise ValueError(f"désordre ou séparateur perdu à la ligne {seen+1}")
            previous = row
            seen += 1
    if seen != count:
        raise ValueError(f"nombre de lignes {seen} au lieu de {count}")
    return digest.hexdigest()


def measure(binary, args, source, manifest, work):
    executable_sha256 = hashlib.sha256(binary.read_bytes()).hexdigest()
    size = manifest["size"]
    # The one-pass profile was measured on this corpus with 81 runs.
    one_pass = args.stream and args.budget=="2G" and args.batch>=128 and size<=160*(1<<30) and args.workers==1
    needed = size + size//10 if one_pass else size*2
    if shutil.disk_usage(work).free < needed + RESERVE:
        raise ValueError("espace insuffisant pour temporaires, sortie et marge")
    with tempfile.TemporaryDirectory(dir=work) as temp:
        runs = Path(temp) / "runs"
        runs.mkdir()
        output = Path(temp) / "output"
        cmd = [str(binary), "-S", args.budget, "--parallel="+str(args.workers),
               "--batch-size="+str(args.batch), "-T", str(runs), str(source)]
        files = {}
        peak = 0
        with output.open("xb") as out:
            start = time.monotonic()
            p = subprocess.Popen(cmd, stdout=subprocess.PIPE if args.stream else out, env=dict(os.environ, LC_ALL="C", NOVA_SORT_METRICS=str(Path(temp)/"metrics")))
            done = threading.Event()
            faults = []
            def observe():
                nonlocal peak
                try:
                    while not done.is_set():
                        current = 0
                        for item in runs.iterdir():
                            try:
                                stat = item.stat()
                            except FileNotFoundError:
                                continue
                            files[item.name]=max(files.get(item.name,0),stat.st_size)
                            current += stat.st_blocks * 512
                        peak = max(peak, current)
                        if shutil.disk_usage(work).free < RESERVE:
                            raise RuntimeError("marge disque atteinte en cours de tri")
                        done.wait(0.05)
                except Exception as error:
                    faults.append(error)
                    p.kill()
            streamed = []
            def consume():
                try:
                    with p.stdout:
                        streamed.append(validate(p.stdout,manifest["rows"]))
                except Exception as exc:
                    faults.append(exc)
                    p.kill()
            consumer = threading.Thread(target=consume)
            if args.stream:
                consumer.start()
            watcher = threading.Thread(target=observe)
            watcher.start()
            try:
                _, status, usage = os.wait4(p.pid, 0)
                elapsed = time.monotonic() - start
                p.returncode = os.waitstatus_to_exitcode(status)
            finally:
                done.set()
                watcher.join()
                if args.stream:
                    consumer.join()
            if faults:
                raise faults[0]
        if p.returncode or list(runs.iterdir()):
            raise RuntimeError(f"statut {p.returncode} ou temporaires résiduels")
        check = time.monotonic()
        if args.stream:
            sha=streamed[0]
        else:
            with output.open("rb") as stream:
                sha=validate(stream,manifest["rows"])
        metrics = Path(temp)/"metrics"
        return dict(executable_sha256=executable_sha256, sampled_max_size_by_temp=files, destination="pipe avec validation" if args.stream else "fichier APFS", metrics=json.loads(metrics.read_text()) if metrics.exists() else None, command=cmd, architecture=platform.machine(), os=platform.platform(),
                    elapsed_s=elapsed, cpu_user_s=usage.ru_utime, cpu_system_s=usage.ru_stime,
                    rss_bytes=usage.ru_maxrss, mib_s=size/(1<<20)/elapsed,
                    io_in_blocks=usage.ru_inblock, io_out_blocks=usage.ru_oublock,
                    major_faults=usage.ru_majflt, swaps=usage.ru_nswap,
                    sampled_temp_allocated_bytes=peak, observed_temp_names=len(files),
                    output_sha256=sha, output_bytes=size if args.stream else output.stat().st_size,
                    rows=manifest["rows"], validation_s=time.monotonic()-check)


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--engine", choices=["gnu","nova","both"], default="both")
    p.add_argument("--mib", type=int, default=8)
    p.add_argument("--budget", default="1M")
    p.add_argument("--workers", type=int, default=1)
    p.add_argument("--batch", type=int, default=16)
    p.add_argument("--repeat", type=int, default=3)
    p.add_argument("--distribution", choices=["random","sorted","reverse","duplicates"],default="random")
    p.add_argument("--work", type=Path, default=Path(os.environ["TMPDIR"]))
    p.add_argument("--manifest", type=Path, help="réutiliser un corpus déjà généré")
    p.add_argument("--candidate", type=Path,default=CANDIDATE)
    p.add_argument("--output",type=Path)
    p.add_argument("--stream",action="store_true", help="vérifier et hacher dans un pipe pendant le tri")
    p.add_argument("--above-ram",action="store_true")
    args = p.parse_args()
    ram=int(subprocess.check_output(["sysctl","-n","hw.memsize"]))
    size=args.mib << 20
    if args.repeat < 1 or size <= 0:
        p.error("taille et nombre de passages positifs requis")
    with tempfile.TemporaryDirectory(dir=args.work) as temp:
        if args.manifest:
            manifest=json.loads(args.manifest.read_text())
            source=Path(manifest["path"]).resolve()
            size=manifest["size"]
            with source.open("rb") as f:
                if hashlib.file_digest(f,"sha256").hexdigest()!=manifest["input_sha256"]:
                    raise ValueError("empreinte du corpus incohérente")
            if source.stat().st_size != size:
                raise ValueError("taille du manifeste incohérente")
        else:
            source=Path(temp) / "input"
        if (size > ram) != args.above_ram:
            p.error("utiliser --above-ram exactement pour un palier supérieur à la RAM")
        if not args.manifest:
            manifest=generate(source,size,args.distribution)
        report=dict(ram=ram, corpus=manifest, cache="non purgé", results=[], summary={})
        reference=None
        for _ in range(args.repeat):
            for name,exe in [("gnu",GNU),("nova",args.candidate)]:
                if args.engine not in ("both",name):
                    continue
                item=measure(exe,args,source,manifest,args.work)
                item["engine"]=name
                report["results"].append(item)
                if reference is not None and reference!=item["output_sha256"]:
                    raise ValueError("empreintes divergentes")
                reference=item["output_sha256"]
        for name in ("gnu","nova"):
            times=[v["elapsed_s"] for v in report["results"] if v["engine"]==name]
            if times:
                report["summary"][name]=dict(median_s=statistics.median(times),min_s=min(times),max_s=max(times))
        result=json.dumps(report,indent=2)
        if args.output:
            args.output.write_text(result+"\n")
        print(result)


if __name__=="__main__":
    main()
