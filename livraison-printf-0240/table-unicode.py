"""Reconstituer les transitions de la classe imprimable Darwin en UTF-8."""

import argparse
import ctypes
import locale
from pathlib import Path

TABLE = Path(__file__).resolve().parents[1] / "components/printf/src/printable.rs"


def generate():
    locale.setlocale(locale.LC_CTYPE, "en_US.UTF-8")
    printable = ctypes.CDLL(None).iswprint
    printable.argtypes = [ctypes.c_uint32]
    previous = False
    boundaries = []
    for scalar in range(0x110000):
        current = bool(printable(scalar))
        if current != previous:
            boundaries.append(f"0x{scalar:x}")
            previous = current
    rows = [
        "    " + ", ".join(boundaries[i : i + 8]) + ","
        for i in range(0, len(boundaries), 8)
    ]
    return (
        "// Transitions mesurées par livraison-printf-0240/table-unicode.py.\n"
        "#[rustfmt::skip]\npub const BOUNDARIES: &[u32] = &[\n"
        + "\n".join(rows)
        + "\n];\n"
    )


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--regenerer", action="store_true")
    args = p.parse_args()
    table = generate()
    if args.regenerer:
        TABLE.write_text(table)
    elif TABLE.read_text() != table:
        raise SystemExit("La classe imprimable du Mac a changé.")
    print("Classe imprimable relue pour 1 114 112 valeurs.")
