"""Échantillonnage séparé des chronométrages, sur deux charges et le seul processus awk."""
import argparse
from pathlib import Path
import os
import subprocess
import tempfile
from bench import corpus, correct


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("executable", type=Path)
    ap.add_argument("sortie", type=Path)
    args = ap.parse_args()
    args.sortie.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(dir=os.environ["NOVAOS_TMPDIR"]) as tmp:
        path, out = Path(tmp)/"input", Path(tmp)/"stdout"
        n = 1024*8192
        corpus(path,n)
        for name,prog,expected in [
                ("comptage","END{print NR}",str(n)),
                ("tableau","{s[$1]+=$2} END{for(k in s) print k,s[k]}","map")]:
            with out.open("wb") as stdout, subprocess.Popen([args.executable.resolve(),prog,path],
                    stdout=stdout, stderr=subprocess.PIPE, env={**os.environ,"LC_ALL":"C"}) as proc:
                sample = subprocess.run(["/usr/bin/sample",str(proc.pid),"1","3","-file",
                                         args.sortie/(name+".txt")],capture_output=True)
                _, err = proc.communicate(timeout=30)
                assert proc.returncode == 0 and not err and correct(out,expected,n,False)
                if sample.returncode: raise RuntimeError(sample.stderr.decode(errors="replace"))


if __name__ == "__main__":
    main()
