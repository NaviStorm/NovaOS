"""Joint les observations Mach-O, ELF et nsh sans extrapoler entre cibles."""
import argparse
import base64
import csv
import gzip
import hashlib
import json
from pathlib import Path

HERE = Path(__file__).resolve().parents[1]
PROFILS = ("aarch64-512", "aarch64-4096", "x86_64-512", "x86_64-4096")


def load(path):
    with gzip.open(path, "rt") as stream:
        return json.load(stream)


def words(row):
    if "argv" in row:
        return [base64.b64decode(word).decode("utf-8", "backslashreplace") for word in row["argv"]]
    return row.get("args", ["-w", row.get("option", ""), "unused"])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("preuves", type=Path)
    parser.add_argument("sortie", type=Path)
    args = parser.parse_args()
    root = args.preuves
    with gzip.open(root / "options.jsonl.gz", "rt") as stream:
        host = [json.loads(line) for line in stream]
    native = {}
    shells = {}
    for name in PROFILS:
        sdk = load(root / f"sdk-{name}.json.gz")
        elf = HERE / "bin" / f"seq-{name.split('-')[0]}.elf"
        assert sdk["elf"] == hashlib.sha256(elf.read_bytes()).hexdigest()
        assert all(row["ok"] for row in sdk["cas"])
        native[name] = {row["id"]: row for row in sdk["cas"]}
        shell = load(root / f"nsh-{name}.json.gz")
        assert shell["avant"] == shell["apres"]
        shells[name] = shell["cas"]
    assert all(row["ok"] for row in host)
    with gzip.open(args.sortie, "wt", newline="") as stream:
        writer = csv.writer(stream, delimiter="\t", lineterminator="\n")
        writer.writerow(["essai", "locale", "arguments JSON", "Darwin ARM64", *PROFILS])
        for row in host:
            statuses = []
            for name in PROFILS:
                item = native[name].get(row["id"])
                statuses.append(f"{item['oracle']} / {item['statut']}" if item else "non exercé")
            writer.writerow([row["id"], row.get("locale", "C"), json.dumps(words(row), ensure_ascii=False),
                             f"{row['cas']} / {row['nova'][0]}", *statuses])
        for key, item in native[PROFILS[0]].items():
            if isinstance(key, str):
                writer.writerow([key, "ELF", key, "", *[native[n][key]["statut"] for n in PROFILS]])
        for i, (tag, cmd, _) in enumerate(shells[PROFILS[0]]):
            assert all(shells[n][i][:2] == [tag, cmd] for n in PROFILS)
            writer.writerow([f"nsh-{i+1}", tag, cmd, "", *[shells[n][i][2] for n in PROFILS]])


if __name__ == "__main__":
    main()
