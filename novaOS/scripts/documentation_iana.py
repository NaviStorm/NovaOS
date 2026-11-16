"""Produit les listes embarquées depuis une copie locale du Language Subtag Registry."""
import argparse
from pathlib import Path
import textwrap


def values(record, field):
    return [line.removeprefix(field + ": ") for line in record.splitlines()
            if line.startswith(field + ": ")]


def expand(value):
    if ".." not in value:
        return [value]
    first, last = value.lower().split("..")
    codes = []
    current = list(first)
    while True:
        code = "".join(current)
        codes.append(code)
        if code == last:
            return codes
        for i in reversed(range(len(current))):
            if current[i] != "z":
                current[i] = chr(ord(current[i]) + 1)
                break
            current[i] = "a"


def render(registry):
    header, *records = registry.split("%%")
    date, = values(header, "File-Date")
    groups = {kind: [] for kind in ("script", "region", "variant")}
    for record in records:
        kind, = values(record, "Type")
        if kind == "extlang" and "fr" in values(record, "Prefix"):
            raise ValueError("Le registre contient désormais un extlang français.")
        if kind in groups:
            tag, = values(record, "Subtag")
            groups[kind].extend(expand(tag.lower()))
    lines = [f"// Registre IANA du {date}. Généré par novaOS/scripts/documentation_iana.py."]
    for kind, codes in groups.items():
        data = "\n".join(textwrap.wrap(" ".join(sorted(codes)), width=96))
        lines.append(f'pub(super) const {kind.upper()}: &str = "\n{data}\n";')
    return "\n".join(lines) + "\n"


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("registry", type=Path)
    print(render(parser.parse_args().registry.read_text(encoding="utf-8")), end="")
