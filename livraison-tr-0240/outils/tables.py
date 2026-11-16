"""Figer les classes et conversions observées sur les 256 octets de Darwin."""
import os
import subprocess

classes = "alnum alpha blank cntrl digit graph lower print punct space upper xdigit".split()
data = bytes(range(256))
print("// Observations de gtr 9.12. Régénération avec livraison-tr-0240/outils/tables.py.")
print("pub const NAMES: [&[u8]; 12] = [" + ", ".join(f'b"{c}"' for c in classes) + "];")
for label, locale in [("C", "C"), ("UTF8", "en_US.UTF-8")]:
    def run(*args):
        return subprocess.run(["/opt/homebrew/bin/gtr", *args], input=data,
                              stdout=subprocess.PIPE, check=True,
                              env=dict(os.environ, LC_ALL=locale)).stdout
    print(f"pub const {label}: [u16; 256] = [")
    sets = [set(run("-cd", f"[:{name}:]")) for name in classes]
    print(",".join(str(sum(1 << i for i, members in enumerate(sets) if b in members)) for b in data))
    print("];")
    for src, dst in [("lower", "upper"), ("upper", "lower")]:
        print(f"pub const {label}_{dst.upper()}: [u8; 256] = [")
        print(",".join(map(str, run(f"[:{src}:]", f"[:{dst}:]"))))
        print("];")
