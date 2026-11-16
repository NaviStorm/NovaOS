"""Petits témoins différentiels et injection de pannes sur le volume de travail."""
import concurrent.futures
import json
import os
from pathlib import Path
import subprocess
import tempfile

from options import GNU, ROOT, brief, invoke


def run():
    nova = ROOT / "artifacts/darwin-arm64/sort"
    work = Path(os.environ["TMPDIR"])
    results = {}
    for name, flags, data, locale in [
        ("debug-cle-numerique", ["--debug", "-k2n"], b"x 2\n", "C"),
        ("debug-largeur-utf8", ["--debug", "-k1,1"], "é z\n".encode(), "en_US.UTF-8"),
        ("argv-non-utf8", [b"\xff"], b"", "C"),
        ("aleatoire-locale", ["-R", "--random-source=seed"], "a\nA\nè\né\nx\n".encode(), "en_US.UTF-8"),
        ("check-plusieurs-entrees", ["-c", "a", "b"], b"", "C"),
    ]:
        results[name] = {"args": [os.fsdecode(v) for v in flags], "input_hex": data.hex(),
                         "locale": locale}
        for engine, exe in [("gnu", GNU), ("nova", nova)]:
            results[name][engine] = brief(invoke(exe, flags, data, work, locale))
    with tempfile.TemporaryDirectory(dir=work) as folder:
        parent = Path(folder)
        codec = parent / "codec"
        codec.write_text('#!/bin/sh\nif [ "$1" = "-d" ]; then exit 7; fi\nexec /bin/cat\n')
        codec.chmod(0o700)
        data = b"z\na\n" * 8000
        for name, exe in [("gnu", GNU), ("nova", nova)]:
            tmp = parent / name
            tmp.mkdir()
            def sort(extra):
                return subprocess.run(["sort", "-S16K", "--batch-size=2", "-T", tmp, *extra],
                                      executable=exe, input=data, capture_output=True,
                                      env={**os.environ, "LC_ALL": "C"}, timeout=90)
            broken = sort(["--compress-program", codec])
            results["decompression-" + name] = dict(status=broken.returncode,
                stderr=broken.stderr.decode(errors="backslashreplace"), restes=len(list(tmp.iterdir())))
            assert broken.returncode != 0 and not list(tmp.iterdir())
            with concurrent.futures.ThreadPoolExecutor(2) as pool:
                calls = [pool.submit(sort, []) for _ in range(2)]
                copies = [future.result() for future in calls]
            assert all(p.returncode == 0 and p.stdout == b"a\n"*8000+b"z\n"*8000 for p in copies)
            results["simultanes-" + name] = dict(statuts=[p.returncode for p in copies],
                octets=len(copies[0].stdout), restes=len(list(tmp.iterdir())))
    print(json.dumps(results,ensure_ascii=True,indent=2))


if __name__ == "__main__":
    run()
