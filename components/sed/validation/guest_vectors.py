"""Génère les témoins sur flux mémoire embarqués exclusivement dans le profil qualification."""
from pathlib import Path
import json
import sys
from differential import cases

IDS = """
no-match address-number last address-regex range negated zero-range step relative BRE-captures
BRE-backref ampersand second-global global print-first delete restart next join hold hold-append
exchange translate label branch T append insert change number list clear version-instruction comment
q Q caseflag multiline replacement-case posix-longest witness-ERE witness-N witness-no-match witness-q
invalid-utf8 invalid-regex-dot null null-multi unterminated empty long-ere r-alias binary line-length
unicode-dot line-no-final-p short-width short-unbuffered long-binary tilde-range address-I address-M
address-previous comment-ordinary
""".split()

def literal(data):
    def char(b):
        if b in (34, 92): return "\\" + chr(b)
        if b == 10: return "\\n"
        if 32 <= b <= 126: return chr(b)
        return "\\x%02x" % b
    return 'b"' + "".join(map(char,data)) + '"'

def main():
    root = Path(__file__).resolve().parents[1]
    rows = {r['name']:r for r in json.loads((root/'validation/results/C.json').read_text())['rows']}
    inputs = {name:data for name, _, data in cases()}
    lines = ["// Généré par validation/guest_vectors.py depuis la référence GNU sed 4.10 en locale C.", "&["]
    for name in IDS:
        row = rows[name]
        args = ", ".join(literal(s.encode()) for s in row["args"])
        ref = row["reference"]
        lines.append('    ("%s", &[%s], %s, %s, %d),' % (
            name, args, literal(inputs[name]), literal(bytes.fromhex(ref["stdout"]["hex"])), ref["code"]))
    (root/"src/guest_vectors.in").write_text("\n".join([*lines, "]", ""]))

if __name__ == "__main__":
    main()
