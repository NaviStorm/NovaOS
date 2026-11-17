"""Vecteurs du véritable ELF sur trois ports IPC, sans délégation de fichiers."""

import json
import os
import subprocess
import tempfile
from pathlib import Path
from options import GNU


def generate(directory, report):
    conv = "ascii ebcdic ibm block unblock lcase ucase sparse swab sync excl nocreat notrunc noerror fdatasync fsync".split()
    flags = "append binary cio direct directory dsync noatime nocache noctty nofollow nolinks nonblock sync text fullblock count_bytes skip_bytes seek_bytes".split()
    unavailable = set(
        "append directory dsync nocache noctty nofollow nonblock sync".split()
    )
    rejected = "dd: opération indisponible pour ce profil\n".encode()
    cases = []

    def add(args, blocked=False):
        words = ["status=none", *args]
        # Même entrée synthétique que l’acteur SDK. Fin de flux après la trame.
        ref = subprocess.run(
            ["dd", *words],
            executable=GNU,
            input=bytes(range(8)),
            capture_output=True,
            env=dict(os.environ, LC_ALL="C"),
            check=False,
        )
        rc, out, err = ref.returncode, ref.stdout, ref.stderr.replace(b"gdd: ", b"dd: ")
        cases.append(
            dict(
                args=words,
                gnu=[rc, out.hex(), err.hex()],
                attendu=[1, "", rejected.hex()]
                if blocked
                else [rc, out.hex(), err.hex()],
                refus_natif=blocked,
            )
        )

    for v in conv:
        for extra in [[], ["ibs=3", "obs=7", "cbs=4"]]:
            add(["conv=" + v, *extra], v in ("fsync", "fdatasync"))
    for key in ("iflag", "oflag"):
        for v in flags:
            add([key + "=" + v], v in unavailable)
    for words in [
        "",
        "--",
        "-- bs=4",
        "-- --",
        "-- --help",
        "-h",
        "--invalid",
        "bs=3",
        "bs=100m",
        "bs=100M count=0",
        "ibs=2",
        "obs=5",
        "cbs=3",
        "count=2B",
        "skip=2B",
        "iseek=2B",
        "seek=1",
        "oseek=1B",
        "conv=swab,ucase",
        "conv=lcase,ucase",
        "cbs=3 conv=block,unblock",
        "conv=ascii,ebcdic",
        "conv=nocreat,excl",
        "count=1x2B",
        "count=0x8",
        "status=noxfer",
        "iflag=fullblock,count_bytes count=5",
        "oflag=seek_bytes oseek=0",
        "bs=50000000000000000000",
        "ibs=7 bs=3 obs=1",
        "conv=bad",
        "status=oops",
        "iflag=bad",
        "oflag=bad",
    ]:
        add(words.split(), words.startswith(("seek=1", "oseek=1B")))
    # Observer les deux accès hôtes dans un répertoire jetable, avec les mêmes octets.
    for operand, diagnostic in [
        ("if=absent", "dd: accès refusé\n".encode()),
        ("of=sortie", rejected),
    ]:
        words = ["status=none", operand]
        with tempfile.TemporaryDirectory(
            prefix="dd-elf-gnu-", dir=os.environ["TMPDIR"]
        ) as tmp:
            ref = subprocess.run(
                ["dd", *words],
                executable=GNU,
                input=bytes(range(8)),
                capture_output=True,
                cwd=tmp,
                env=dict(os.environ, LC_ALL="C"),
            )
            effect = Path(tmp) / "sortie"
            cases.append(
                dict(
                    args=words,
                    gnu=[
                        ref.returncode,
                        ref.stdout.hex(),
                        ref.stderr.replace(b"gdd: ", b"dd: ").hex(),
                    ],
                    fichier_gnu=effect.read_bytes().hex() if effect.exists() else None,
                    attendu=[1, "", diagnostic.hex()],
                    refus_natif=True,
                )
            )

    def literal(hexadecimal):
        return (
            'b"'
            + "".join(
                "\\x" + hexadecimal[i : i + 2] for i in range(0, len(hexadecimal), 2)
            )
            + '"'
        )

    rust = []
    for c in cases:
        rc, out, err = c["attendu"]
        args = "&[" + ",".join(json.dumps(s) for s in c["args"]) + "]"
        rust.append(
            "(" + args + "," + str(rc) + "," + literal(out) + "," + literal(err) + ")"
        )
    (directory / "dd-exact.rs").write_text("&[\n" + ",\n".join(rust) + "\n]")
    report.write_text(json.dumps(cases, indent=2) + "\n")
    return len(cases)
