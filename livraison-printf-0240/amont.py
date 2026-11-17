"""Rejouer les sept tests GNU non modifiés avec le harnais du gitlink gnulib."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
from options import GNU, NOVA, TMP


def main():
    import argparse

    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("sortie", type=Path)
    args = p.parse_args()
    out = args.sortie.resolve()
    out.mkdir(exist_ok=True, parents=True)
    src = Path(os.environ.get("GNU_COREUTILS_SRC", TMP.parent / "sort-arc027-gnu"))
    commit = "c0f8514d989184921d9b12a4d103a7b23abc5af8"
    assert (
        subprocess.check_output(
            ["git", "-C", src, "rev-parse", "HEAD"], text=True
        ).strip()
        == commit
    )
    gnulib = subprocess.check_output(
        ["git", "-C", src, "rev-parse", "HEAD:gnulib"], text=True
    ).strip()
    work = Path(tempfile.mkdtemp(dir=TMP, prefix="amont-"))
    harness = work / "source"
    (harness / "tests").mkdir(parents=True)
    (harness / "tests/init.sh").write_bytes(
        subprocess.check_output(
            ["git", "-C", src / "gnulib", "show", gnulib + ":tests/init.sh"]
        )
    )
    shutil.copyfile(src / "init.cfg", harness / "init.cfg")
    rows = []
    for product, exe in [("GNU", GNU), ("Nova", NOVA)]:
        base = work / product
        programs = base / "src"
        programs.mkdir(parents=True)
        (programs / "printf").symlink_to(exe)
        limits = programs / "getlimits"
        limits.write_text(
            "#!/bin/sh\necho INT_MAX=2147483647 INT_OFLOW=2147483648 INT_UFLOW=-2147483649 INTMAX_MAX=9223372036854775807 INTMAX_OFLOW=9223372036854775808\n"
        )
        limits.chmod(0o755)
        env = {
            **os.environ,
            "PATH": str(programs)
            + ":/opt/homebrew/opt/coreutils/libexec/gnubin:"
            + os.environ["PATH"],
            "built_programs": "printf",
            "srcdir": str(harness),
            "abs_srcdir": str(src),
            "LOCALE_FR_UTF8": "fr_FR.UTF-8",
            "LC_ALL": "C",
            "TMPDIR": str(TMP) + "/",
            "VERBOSE": "yes",
        }
        for script in sorted((src / "tests/printf").iterdir()):
            cmd = (
                ["perl", "-I" + str(src / "tests"), "-MCuSkip", "-MCoreutils"]
                if script.suffix == ".pl"
                else ["/bin/sh"]
            )
            test = subprocess.run(
                [*cmd, script],
                env=env,
                cwd=base,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                timeout=90,
            )
            log = out / (product + "-" + script.name + ".log")
            log.write_bytes(test.stdout)
            rows.append(dict(produit=product, test=script.name, code=test.returncode))
            print(product, script.name, test.returncode)
    (out / "amont.json").write_text(json.dumps(rows, indent=2) + "\n")
    return any(r["code"] not in (0, 77) for r in rows)


if __name__ == "__main__":
    raise SystemExit(main())
