"""Cas de flux Darwin. Mesure par wait4 du consommateur, y compris pour le vrai pipe cat."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
from bench import digest, run


def piped(exe, prog, path, output, errors):
    with output.open("wb") as out, errors.open("wb") as err:
        start = time.monotonic()
        with subprocess.Popen(["/bin/cat", path], stdout=subprocess.PIPE) as cat:
            proc = subprocess.Popen([exe, prog], stdin=cat.stdout, stdout=out, stderr=err,
                                    env={**os.environ, "LC_ALL":"C"})
            cat.stdout.close()
            _, status, usage = os.wait4(proc.pid, 0)
            proc.returncode = os.waitstatus_to_exitcode(status)
            producer = cat.wait()
        seconds = time.monotonic() - start
    return dict(status=proc.returncode, cat=producer, seconds=seconds, user=usage.ru_utime,
                system=usage.ru_stime, rss_bytes=usage.ru_maxrss, stderr=errors.read_text())


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("oracle", type=Path)
    ap.add_argument("candidate", type=Path)
    ap.add_argument("results", type=Path)
    args = ap.parse_args()
    rows = []
    with tempfile.TemporaryDirectory(dir=os.environ["NOVAOS_TMPDIR"]) as work:
        path, output, errors = [Path(work)/n for n in ("input","out","err")]
        for name, source, prog, expected, count, pipe in [
            ("sans-lf", b"abc", "{print NR,length($0)}", b"1 3\n", 1, False),
            ("frontiere-regex", b"x"*8191+b"::y", 'BEGIN{RS="::"}{print length($0)}', b"8191\n1\n", 2, False),
            ("ligne-1M", b"x"*(1<<20)+b"z", "{print length($0),substr($0,length($0))}", b"1048577 z\n",1,False),
            ("ligne-16M", b"x"*(16<<20)+b"z", "{print length($0),substr($0,length($0))}", b"16777217 z\n",1,False),
            ("pipe-64M", (b"x"*127+b"\n")*(1<<19), "END{print NR}", b"524288\n",1<<19,True)]:
            path.write_bytes(source)
            for binary in [args.oracle.resolve(), args.candidate.resolve()]:
                for repeat in range(4):
                    t = piped(binary, prog, path, output, errors) if pipe else run(binary, [prog], path, output, errors)
                    ok = t["status"] == 0 and t.get("cat",0) == 0 and not t["stderr"] and output.read_bytes() == expected
                    rows.append(dict(name=name, executable=str(binary), sha256=digest(binary), repeat=repeat,
                                     bytes=len(source), input_sha256=digest(path), prog=prog, records=count,
                                     mib_s=len(source)/(1<<20)/t["seconds"], records_s=count/t["seconds"],
                                     expected=expected.hex(), output_sha256=digest(output), ok=ok, **t))
                    if not ok: break
    args.results.write_text(json.dumps(rows, indent=1)+"\n")
    if not all(r["ok"] for r in rows): raise SystemExit(1)


if __name__ == "__main__":
    main()
