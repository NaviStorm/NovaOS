"""Rejouer le différentiel publié et les transitions affectées par les tampons."""
import argparse
import json
import os
from pathlib import Path
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "livraison-realpath-0240/lab"))
import options


def chain_cases():
    # Cibles plus longues que le premier tampon, puis plus courtes dans le même lot.
    parent = Path("a" * 150) / ("b" * 150)
    parent.mkdir(parents=True)
    (parent / "fin").touch()
    Path("long").symlink_to(parent)
    Path("court").symlink_to("long")
    Path("imbrique").symlink_to("court/../" + parent.name)
    Path("simple").symlink_to(".")
    names = ["imbrique/fin", "court/..", "long/../", "simple/simple/simple",
             "broken/../tree", "cycle", "cycle3-0/a", "missing/", "tree/file/..",
             "secret/child", "", "tree/file", "court/fin"]
    for mode in ["-E", "-e", "-m"]:
        for links in ["-L", "-P", "-s"]:
            yield [mode, links, "--relative-to=.", "-z", *names]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("candidat", type=Path)
    parser.add_argument("dossier", type=Path)
    args = parser.parse_args()
    candidate = args.candidat.resolve(strict=True)
    dest = args.dossier.resolve()
    dest.mkdir(parents=True, exist_ok=True)
    published = options.NOVA
    options.NOVA = candidate
    # Le banc historique évalue aussi la grammaire, les locales et les pseudo-terminaux.
    sys.argv = ["options.py", "complet", str(dest / "options.json")]
    try:
        options.main()
    except SystemExit as status:
        if status.code:
            raise
    records = []
    previous = Path.cwd()
    with tempfile.TemporaryDirectory(dir=os.environ["TMPDIR"]) as tmp:
        os.chdir(tmp)
        try:
            options.fixtures()
            for argv in chain_cases():
                values = {key: options.observe(exe, argv, {"LC_ALL": "C"})
                          for key, exe in [("gnu", options.GNU), ("publie", published),
                                           ("candidat", candidate)]}
                records.append(dict(argv=argv, **values))
        finally:
            Path("secret").chmod(0o700)
            os.chdir(previous)
    (dest / "chaines.json").write_text(json.dumps(records, indent=1) + "\n")
    assert all(r["gnu"] == r["publie"] == r["candidat"] for r in records)
    print(f"{len(records)} lots mixtes conformes sur les trois exécutables.")


if __name__ == "__main__":
    main()
