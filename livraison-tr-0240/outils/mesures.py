"""Débit sur les octets lus. Un échauffement et au moins trois passages en ordre alterné."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import statistics
import subprocess
import tempfile
import time

ROOT=Path(__file__).resolve().parents[1]
BIN = {
    "Nova": os.environ.get("NOVA_TR_BIN", str(ROOT / "bin/tr")),
    "GNU": os.environ.get("GNU_TR_BIN", "/opt/homebrew/bin/gtr"),
}
MODES={"Traduction":["a-z","A-Z"],"Suppression":["-d","X"],"Compression":["-s","aX"],"Suppr. + compr.":["-ds","X","a"]}
# Motifs fixes, répétés exactement n fois. Le préfixe du dernier texte allonge la première ligne.
DATA=[("1 M lignes",b"",b"aB0Xz9!\n",1_000_000),
      ("10 M lignes",b"",b"aaaXXXX\n",10_000_000),
      ("65 M lignes",b"a"*(1<<20),b"aaXaXXX\n",65_000_000),
      ("32 Mio binaire",b"",bytes(range(256)),131072)]

LOG_LINE = b"2026-11-17T08:30:00Z info service=tr client=abc-123\n"

def workloads(large):
    if large:
        # Conserver les seize premières lignes consommées par le témoin SDK.
        for dataset in DATA:
            yield (*dataset, list(MODES))
        for n in (1_000_000, 10_000_000, 65_000_000):
            yield (f"{n // 1_000_000} M lignes journal", b"", LOG_LINE, n, ["Traduction"])
        return
    yield ("100 000 lignes journal", b"", LOG_LINE, 100_000, ["Traduction"])
    yield ("4 096 lignes répétées", b"a" * (1 << 20), b"X" * 2048 + b"a" * 128 + b"\n",
           4096, ["Suppression", "Compression", "Suppr. + compr."])
    yield ("1 048 576 octets", b"", bytes(range(256)), 4096, list(MODES))

def create(path, prefix, pattern, repeats):
    with path.open("xb") as f:
        f.write(prefix)
        block=pattern*8192
        for _ in range(repeats//8192): f.write(block)
        f.write(pattern*(repeats%8192))

def measure(binary,args,path):
    with path.open("rb") as inp, open(os.devnull,"wb") as out:
        start=time.perf_counter_ns()
        p=subprocess.Popen([binary,*args],stdin=inp,stdout=out)
        _,s,r=os.wait4(p.pid,0)
        seconds=(time.perf_counter_ns()-start)*1e-9
        p.returncode=os.waitstatus_to_exitcode(s)
        if p.returncode: raise RuntimeError((binary,args,p.returncode))
    return dict(seconds=seconds,rss_bytes=r.ru_maxrss)

def verify(binary,args,path):
    h=hashlib.sha256()
    size=0
    with path.open("rb") as fd, tempfile.TemporaryFile(dir=os.environ["NOVAOS_TMPDIR"]) as err:
        with subprocess.Popen([binary, *args], stdin=fd, stdout=subprocess.PIPE, stderr=err) as child:
            while chunk := child.stdout.read(1 << 20):
                size += len(chunk)
                h.update(chunk)
            status = child.wait()
        err.seek(0)
        stderr = err.read()
    if status or stderr:
        raise RuntimeError((binary, args, status, stderr[:512]))
    return dict(output=size, sha256_output=h.hexdigest(), status=status, stderr_hex=stderr.hex())


def show(rows):
    table = [("Taille ou lignes", "Traitement", "Nova tr (s)", "GNU tr (s)",
              "Nova tr (Mio/s lus)", "GNU tr (Mio/s lus)", "Entrée (octets)", "Sortie (octets)")]
    for row in rows:
        summaries = row["medians"]
        times = [f"{summaries[k]['seconds']:.4f}" if k in summaries else "—" for k in BIN]
        rates = [f"{summaries[k]['input_mib_s']:.1f}" if k in summaries else "—" for k in BIN]
        table.append([row["size"], row["mode"], *times, *rates, str(row["input"]), str(row["output"])])
    widths=list(map(lambda col:max(map(len,col)),zip(*table)))
    for row in table: print(" | ".join(f"{v:<{w}}" for v,w in zip(row,widths)).rstrip())

def main():
    p=argparse.ArgumentParser(description=__doc__)
    g=p.add_mutually_exclusive_group()
    for flag,helptext in [("macOS","GNU"),("novaOS","Mach-O Nova"),("comparer","GNU et Nova alternés (défaut)")]:
        g.add_argument("--"+flag,action="store_true",help=helptext)
    p.add_argument("--large",action="store_true",help="activer les paliers 1, 10 et 65 M de lignes et le binaire de 32 Mio")
    p.add_argument("--rapport",type=Path,help="nouveau fichier JSON")
    p.add_argument("--passages", type=int, default=3, help="passages mesurés par outil (minimum 3)")
    p.add_argument("--gnu-dabord", action="store_true", help="commencer l’alternance par GNU")
    a=p.parse_args()
    if a.passages < 3:
        p.error("trois passages au minimum")
    selected = ["GNU"] if a.macOS else ["Nova"] if a.novaOS else list(BIN)
    if a.gnu_dabord and len(selected) == 2:
        selected.reverse()
    os.environ["LC_ALL"]="C"
    tmp=Path(os.environ.get("NOVAOS_TMPDIR",str(ROOT.parents[1]/"tmp")))
    os.environ.update(TMPDIR=str(tmp)+"/",NOVAOS_TMPDIR=str(tmp))
    tmp.mkdir(parents=True, exist_ok=True)
    rows=[]
    tools = {key: value for key, value in BIN.items() if key == "GNU" or key in selected}
    report=dict(profile="complet" if a.large else "court",cible="Mach-O Darwin",host=platform.platform(),ram=int(subprocess.check_output(["sysctl","-n","hw.memsize"])),
                cache="échauffé, sans purge",destination="/dev/null",locale="C",rows=rows,passages=a.passages,
                versions={k:subprocess.check_output([v,"--version"]).decode().splitlines()[0] for k,v in tools.items()},
                sha256={k:hashlib.sha256(Path(v).read_bytes()).hexdigest() for k,v in tools.items()})
    if "(GNU coreutils)" not in report["versions"]["GNU"]:
        p.error("la référence doit être GNU tr")
    print("Référence", report["versions"]["GNU"])
    if a.rapport and a.rapport.exists(): p.error("choisir un nouveau rapport")
    with (tmp/"arc027-mesures.lock").open("a") as lock, (tmp/"cut-performances.lock").open("a") as cut, tempfile.TemporaryDirectory(prefix="tr-mesures-",dir=tmp) as work:
        fcntl.flock(lock,fcntl.LOCK_EX)
        fcntl.flock(cut,fcntl.LOCK_EX)
        for label,prefix,pattern,n,modes in workloads(a.large):
            size=len(prefix)+len(pattern)*n
            free=shutil.disk_usage(work).free
            if size>free: raise RuntimeError(f"{free} octets libres pour {size} requis")
            path=Path(work)/"corpus"
            create(path,prefix,pattern,n)
            assert path.stat().st_size==size
            with path.open("rb") as f: digest=hashlib.file_digest(f,"sha256").hexdigest()
            for mode in modes:
                args = MODES[mode]
                expected=verify(BIN["GNU"],args,path)
                entries={k:[] for k in selected}
                row=dict(size=label,mode=mode,args=args,input=size,output=expected["output"],sha256_output=expected["sha256_output"],input_sha256=digest,
                         pattern=pattern.hex(),repeats=n,prefix_a=len(prefix),free=free,samples=entries,order=[],verification={"GNU":expected})
                for k in selected:
                    if k != "GNU":
                        observed = verify(BIN[k], args, path)
                        if observed != expected:
                            raise RuntimeError((label, args, observed, expected))
                        row["verification"][k] = observed
                    measure(BIN[k],args,path)
                for iteration in range(a.passages):
                    for k in (selected if iteration%2==0 else selected[::-1]):
                        entries[k].append(measure(BIN[k],args,path))
                        row["order"].append(k)
                row["medians"] = {}
                for tool, samples in entries.items():
                    seconds = statistics.median(sample["seconds"] for sample in samples)
                    row["medians"][tool] = dict(seconds=seconds, input_mib_s=size / 2**20 / seconds,
                                                peak_rss_bytes=max(sample["rss_bytes"] for sample in samples))
                rows.append(row)
            path.unlink()
    show(rows)
    if a.rapport:
        with a.rapport.open("x", encoding="utf-8") as output:
            json.dump(report, output, indent=2, ensure_ascii=False)
            output.write("\n")

if __name__=="__main__": main()
