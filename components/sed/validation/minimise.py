"""Réduction des écarts générés. Chaque témoin final est rejoué avec l'instantané complet."""
import argparse
import functools
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

from differential import SEED, cases, execute


def signature(reference, observed):
    def diagnostic(result):
        message = bytes.fromhex(result["stderr"]["hex"])
        return re.sub(rb"(char |column |character )\d+", rb"\1N", message)
    return ([key for key in ("stdout", "stderr", "code") if reference[key] != observed[key]],
            reference["code"], observed["code"], diagnostic(reference), diagnostic(observed))


def reduce_one(value, accepts):
    step = max(1, len(value))
    while step:
        offset = 0
        while offset + step <= len(value):
            candidate = value[:offset] + value[offset + step:]
            if accepts(candidate):
                value = candidate
            else:
                offset += 1
        step //= 2
    return value


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("binary", type=Path)
    ap.add_argument("results", type=Path)
    args = ap.parse_args()
    bins = [shutil.which("gsed"), str(args.binary.resolve(strict=True))]
    fixtures = {name: (argv, data) for name, argv, data in cases()}
    temporary = Path(os.environ["NOVAOS_TMPDIR"])
    with tempfile.TemporaryDirectory(dir=temporary) as wd:
        for loc in ("C", "en_US.UTF-8"):
            rows = json.loads((args.results / (loc + ".json")).read_text())["rows"]
            @functools.lru_cache(maxsize=4096)
            def run(script, data):
                outputs = []
                for binary in bins:
                    p = subprocess.run([binary, "--sandbox", "-nE", script], input=data, capture_output=True,
                                       cwd=wd, env={**os.environ, "LC_ALL": loc, "LANG": "en-US"}, timeout=3)
                    outputs.append(dict(code=p.returncode, stdout={"hex": p.stdout.hex()}, stderr={"hex": p.stderr.hex()}))
                return signature(*outputs)
            reduced = []
            for row in rows:
                if not row["name"].startswith("fuzz-") or not row["diff"]:
                    continue
                argv, data = fixtures[row["name"]]
                script = argv[-1]
                expected = signature(row["reference"], row["observed"])
                # Réduire uniquement la regex (les commandes générées et --sandbox restent fixes).
                left, regex, right = script.split("/", 2)
                build = lambda rx: left + "/" + rx + "/" + right
                data = reduce_one(data, lambda d: run(script, d) == expected)
                regex = reduce_one(regex, lambda rx: run(build(rx), data) == expected)
                script = build(regex)
                data = reduce_one(data, lambda d: run(script, d) == expected)
                reduced_argv = ["--sandbox", "-nE", script]
                ref, got = [execute(b, reduced_argv, data, loc, temporary) for b in bins]
                assert signature(ref, got) == expected and ref["files"] == got["files"]
                reduced.append(dict(name=row["name"], args=reduced_argv, input=data.hex(), diff=row["diff"],
                                    reference={k:v for k,v in ref.items() if k != "files"},
                                    observed={k:v for k,v in got.items() if k != "files"}, files_identical=True))
            (args.results / ("minimal-" + loc + ".json")).write_text(json.dumps(dict(locale=loc, seed=SEED, rows=reduced),
                                                                                  ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
