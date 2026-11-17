"""Différentiel Darwin. Chaque invocation travaille dans une racine privée neuve."""

import argparse
import base64
import gzip
import hashlib
import itertools
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time

HERE = Path(__file__).resolve().parent
GNU = Path(os.getenv("GNU_DD", "/opt/homebrew/bin/gdd")).resolve()
NOVA = Path(os.getenv("NOVA_DD", HERE / "bin/dd")).resolve()
SOURCE = bytes(range(256)) + b"AaBb\nXY\nEND"


def cases(large):
    for args in [
        "",
        "--",
        "if=src",
        "of=out",
        "bs=3",
        "ibs=3",
        "obs=5",
        "cbs=4",
        "count=2",
        "skip=1",
        "iseek=1",
        "seek=1",
        "oseek=1",
        "if=missing",
        "if=folder",
        "of=folder",
        "if=",
        "of=",
        "-- --",
        "-- --help",
        "-- bs=7 count=2",
        "--help=x",
        "--unknown",
        "-h",
        "-",
        "file",
        "file=src",
        "if=missing if=src",
        "of=missing of=out",
        "ibs=1 bs=7 obs=2 count=2",
        "conv=excl,nocreat",
        "conv=lcase,ucase",
        "conv=ascii,ibm",
        "cbs=2 conv=block,unblock",
        "conv=block,unblock",
        "conv=",
        "conv=bad",
        "iflag=",
        "oflag=",
        "status=",
        "status=bad",
        "status=none,noxfer",
        "if=src skip=999",
        "if=src count=0 skip=4B",
        "if=src seek=1",
        "status=none bs=0",
        "ibs=0",
        "obs=0",
        "cbs=0",
        "count=-1",
        "count=+2",
        "bs=100m",
        "bs=100M count=0",
        "count=0x10",
        "count=00x99999999999999999999999999",
        "count=1x2x4",
        "count=1Bx2x4",
        "count=2Bx4B",
        "count=1xM",
        "bs=10M count=0",
        "count=1 count=0",
        "ibs=5 conv=sync count=2B",
        "seek=2B bs=4 count=0 of=old",
        "seek=4B bs=4 count=0 of=old",
        "seek=3 of=old conv=notrunc",
        "of=out conv=nocreat",
        "of=old conv=excl",
        "of=old oflag=append conv=notrunc",
        "if=sym iflag=nofollow",
        "of=linkout oflag=nofollow",
        "of=absent/out",
        "if=folder iflag=directory count=0",
        "if=src iflag=directory",
        "if=src iflag=nocache count=0",
        "of=out oflag=nocache count=0",
        "iflag=nocache count=0",
        "oflag=nocache count=0",
        "iflag=nocache oflag=nocache",
        "if=src conv=sparse of=out bs=8",
        "if=zeros of=out conv=sparse bs=512",
        "if=zeros of=old conv=sparse,notrunc bs=512",
        "status=progress",
        "status=none",
        "status=noxfer",
    ]:
        yield args.split()
    for word in [
        "if=a'b",
        "if=a b",
        "if=ligne\nsuivante",
        "if=é",
        "if=a'b$c",
        "conv=a'b",
        "conv=deux\nlignes",
        "xx=a'b",
    ]:
        yield [word]
    for prefix in ("--help", "--version"):
        for n in range(3, len(prefix) + 1):
            yield [prefix[:n]]
        yield ["if=absent", prefix]
    for value in "ascii ebcdic ibm block unblock lcase ucase sparse swab sync excl nocreat notrunc noerror fdatasync fsync".split():
        for extra in [[], ["if=src", "of=out", "ibs=3", "obs=7", "cbs=4"]]:
            yield ["conv=" + value, *extra]
    for flag in "append binary cio direct directory dsync noatime nocache noctty nofollow nolinks nonblock sync text fullblock count_bytes skip_bytes seek_bytes".split():
        for key in ["iflag", "oflag"]:
            yield [key + "=" + flag]
            yield [
                key + "=" + flag,
                "if=src",
                "of=out",
                "bs=5",
                "count=8",
                "seek=2",
                "skip=2",
            ]
    for suffix in ["", "B", "w", "b", "c", "bB", "wB", "cB"] + [
        k + unit for k in "KkMGTPEZYRQ" for unit in ["", "B", "iB", "D"]
    ]:
        yield ["count=0" + suffix]
        yield ["count=1" + suffix]
    for (
        value
    ) in "B B1 Bx1 KBB BB KBb KBx x1 1x 1xx1 1ki 1m 1KiBB 1Z 2Qx0 1KBx2".split():
        yield ["count=" + value]
    if large:
        for first, second in itertools.combinations(
            "ascii ebcdic ibm block unblock lcase ucase swab sync sparse notrunc".split(),
            2,
        ):
            for block in [1, 3, 7]:
                yield [
                    "cbs=4",
                    "ibs=" + str(block),
                    "obs=5",
                    "conv=" + first + "," + second,
                ]
        for skip, seek, count in itertools.product(
            ["skip=1", "iseek=2B"], ["seek=1", "oseek=3B"], ["count=1", "count=4B"]
        ):
            yield [skip, seek, count, "ibs=3", "obs=5", "of=old"]


def b64(data):
    return base64.b64encode(data).decode()


def observe(exe, args, root, source=SOURCE, lang="C", pieces=False):
    root.mkdir()
    for name, data in {
        "src": source,
        "zeros": bytes(8193),
        "old": b"original destination" * 3,
    }.items():
        (root / name).write_bytes(data)
    (root / "sym").symlink_to("src")
    (root / "linkout").symlink_to("old")
    (root / "folder").mkdir()
    env = dict(os.environ, LC_ALL=lang, LANG=lang)
    env.pop("POSIXLY_CORRECT", None)
    # argv[0] identique conserve les diagnostics exacts des deux processus.
    proc = subprocess.Popen(
        ["dd", *args],
        executable=exe,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        cwd=root,
        env=env,
    )
    try:
        if pieces:
            for piece in (b"ab", b"cd"):
                proc.stdin.write(piece)
                proc.stdin.flush()
                time.sleep(0.12)
            source = b""
        out, err = proc.communicate(source, timeout=8)
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()
    files = {
        p.name: b64(p.read_bytes())
        for p in root.iterdir()
        if not p.is_symlink() and p.is_file() and p.name not in ("src", "zeros")
    }
    return dict(rc=proc.returncode, stdout=b64(out), stderr=b64(err), files=files)


def compare(pair, doc=False, french=False):
    nova, gnu = pair
    if doc:
        text = base64.b64decode(nova["stdout"])
        expected = (
            b"dd (nova-dd)"
            if b"coreutils) 9.12" in base64.b64decode(gnu["stdout"])
            else ("Copier et convertir" if french else "Copy and convert").encode()
        )
        return (
            nova["rc"] == gnu["rc"] == 0
            and expected in text
            and nova["stderr"] == gnu["stderr"] == ""
            and nova["files"] == gnu["files"]
        )

    def stderr(item):
        raw = base64.b64decode(item["stderr"])
        # Seul le temps et le débit dépendent du chronomètre. Garder le volume et ses unités.
        raw = re.sub(rb"(?m)^gdd: ", b"dd: ", raw)
        return re.sub(rb"(copied), [0-9.e+\-]+ s, [0-9.]+ [A-Za-z]+/s", rb"\1", raw)

    return all(nova[k] == gnu[k] for k in ("stdout", "rc", "files")) and stderr(
        nova
    ) == stderr(gnu)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--complet", action="store_true")
    parser.add_argument("--rapide", action="store_true")
    parser.add_argument("--rapport", type=Path, required=True)
    a = parser.parse_args()
    assert b"9.12" in subprocess.check_output([GNU, "--version"]).splitlines()[0]
    rows = []
    vectors = [(v, "C", False) for v in cases(a.complet)]
    vectors.extend(
        (["--help"], lang, False)
        for lang in ("fr-CA", "fr-Latn-FR", "fr-ZZ-invalid", "fr--CA", "en-US")
    )
    vectors.extend(
        (v.split(), "C", True)
        for v in (
            "ibs=3 obs=3",
            "bs=3",
            "bs=3 iflag=fullblock",
            "bs=3 count=2",
            "bs=3 conv=swab",
        )
    )
    for args, lang, pieces in vectors:
        with tempfile.TemporaryDirectory(
            prefix="dd-options-", dir=os.environ["TMPDIR"]
        ) as folder:
            pair = [
                observe(exe, args, Path(folder) / label, lang=lang, pieces=pieces)
                for label, exe in [("nova", NOVA), ("gnu", GNU)]
            ]
        doc = (
            any(
                x.startswith("--")
                and x != "--"
                and ("--help".startswith(x) or "--version".startswith(x))
                for x in args
            )
            and "--" not in args
        )
        ok = compare(pair, doc, lang in ("fr-CA", "fr-Latn-FR"))
        rows.append(
            dict(
                args=args,
                lang=lang,
                pipe_fragmente=pieces,
                nova=pair[0],
                gnu=pair[1],
                ok=ok,
            )
        )
    report = dict(
        rows=rows,
        binaires={
            str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in (NOVA, GNU)
        },
    )
    with gzip.open(a.rapport, "wt") as f:
        json.dump(report, f, separators=(",", ":"))
    failures = [r["args"] for r in rows if not r["ok"]]
    print(f"{len(rows) - len(failures)}/{len(rows)}", a.rapport)
    if failures:
        print("Écarts", failures[:25])
        raise SystemExit(1)


if __name__ == "__main__":
    main()
