"""Compare les formes GNU en répertoires privés. Sorties, statuts et fichiers conservés."""
import argparse
import json
from pathlib import Path
from differential import probe, sha

# main.c::optab et optlist de la distribution GNU 5.4.1.
OPTIONS = """
f file prog
F field-separator :
v assign x=2
b characters-as-bytes
c traditional
C copyright
d dump-variables ?dump
D debug ?commands
e source BEGIN{print(7)}
E exec prog
g gen-pot
h help
i include inc
I trace
k csv
l load module-inexistant-arc027
L lint ?invalid
M bignum
N use-lc-numeric
n non-decimal-data
o pretty-print ?pretty
O optimize
p profile ?profile
P posix
r re-interval
s no-optimize
S sandbox
t lint-old
V version
T persist ?memory
Y parsedebug
Z locale C
"""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("oracle", type=Path)
    ap.add_argument("candidate", type=Path)
    ap.add_argument("report", type=Path)
    args = ap.parse_args()
    binaries = [str(p.resolve(strict=True)) for p in (args.oracle, args.candidate)]
    fixtures = {"prog": b"BEGIN{print 7}\n", "inc.awk": b"function inc(){return 7}\n",
                "commands": b"quit\ny\n", "in": b"a:b\n"}
    jobs = []
    for line in OPTIONS.strip().splitlines():
        short, long, *field = line.split()
        arg = field[0].lstrip("?") if field else ""
        optional = bool(field and field[0].startswith("?"))
        rest = (["in"] if short in "feE" else ["BEGIN{print inc()}"] if short == "i" else
                ["{print $2}", "in"] if short == "F" else ["BEGIN{print x}"] if short == "v"
                else ["-f", "prog"] if short == "D" else ["BEGIN{print 7}"])
        forms = [["-" + short + arg], ["--" + long + ("=" + arg if arg else "")]]
        if arg and not optional:
            forms += [["-" + short, arg], ["--" + long, arg]]
        if optional:
            forms += [["-" + short], ["--" + long]]
        for form in forms:
            jobs.append((line, form, form + rest))
    for form in [["--"], ["-G"], ["-W", "version"], ["-bh"], ["--vers"]]:
        jobs.append(("syntaxe", form, form + ["BEGIN{print 7}"]))
    records = []
    for row, form, argv in jobs:
        a, b = [probe(exe, argv, b"", fixtures, "C") for exe in binaries]
        records.append(dict(option=row, forme=form, args=argv, gawk=a, awk=b,
                            differences=[k for k in a if a[k] != b[k]]))
    args.report.write_text(json.dumps(dict(executables={exe:sha(Path(exe).read_bytes())
                                                        for exe in binaries},
                                           cas=records), indent=1)+"\n")


if __name__ == "__main__":
    main()
