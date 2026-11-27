"""Régression 4B.1/4C sur Darwin ARM64 et Rosetta, puis compilation no_std sur les deux ISA."""
from pathlib import Path
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/"libs/gestionnaire-pilotes"))
import qualifier as q


def main():
    translated=q.cmd(["arch","-x86_64","/usr/sbin/sysctl","-n","sysctl.proc_translated"])
    if translated!="1": raise RuntimeError("x86_64 sous Rosetta attendu")
    q.log("Révision",q.cmd(["git","rev-parse","HEAD"]))
    q.log("Hôte",q.platform.platform(),q.cmd([q.env["RUSTC"],"-V"]),"Rosetta",translated)
    for target in ("aarch64-apple-darwin","x86_64-apple-darwin"):
        for component,count in ((q.CAT,26),(q.ROOT,28)):
            q.suite(component,target,count)
    for target in ("aarch64-unknown-none-softfloat","x86_64-unknown-none"):
        for component in (q.CAT,q.ROOT): q.object_size(component,target)


if __name__=="__main__": main()
