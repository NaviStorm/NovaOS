"""Rejoue séparément les lots de phase 3D dans un répertoire d'archives choisi."""
import argparse
import os
from pathlib import Path
import subprocess
import sys

from bilan_retrait_duo import BASE, FINAL
from port_qemu import qualification
from retrait_duo import CAS

BANC = Path(__file__).resolve().parent


def lancer(nom, *args):
    subprocess.run([sys.executable, "-B", str(BANC/nom), *map(str,args)],check=True)


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("archive",type=lambda s:Path(s).resolve())
    p.add_argument("lot",choices=("images-duo","duo","instantanes","latences","arm","capacite","transports","ethernet","hote"))
    a=p.parse_args()
    out=a.archive
    tmp=Path(os.environ["NOVAOS_TMPDIR"]).resolve(strict=True)
    wt=tmp.parent
    duo=out/("images-duo-"+FINAL[:7])
    port=out/("images-port-"+FINAL[:7])
    if a.lot=="images-duo":
        with qualification(wt,{}):
            lancer("port_construire_q35.py",duo,"--revision",FINAL,"--phase3d","--cas",*CAS,"deces")
    elif a.lot in ("duo","instantanes","latences"):
        def boot(cas,serie,*options):
            lancer("port_q35_stockage.py",cas,duo/(cas+".elf"),out/serie/cas/"q35",wt/"pilote-virtio-net",
                   wt/"archives-virtio-net",wt/"archives-ethernet-phase3/reproduction-8bd3f9c/limine-enroll",
                   wt/"pilote-virtio-net/novaFS/target/aarch64-apple-darwin/release/novafs",*options)
        if a.lot=="duo":
            for cas in (*CAS,"deces"):
                boot(cas,"qualification-"+FINAL[:7])
        elif a.lot=="instantanes":
            for cas in ("duo-differe","duo-absent"):
                boot(cas,"instantanes-finaux","--instantane")
        else:
            for n in range(2,6):
                ordre=("deces","duo-differe","duo-retrait")
                for cas in ordre if n%2==0 else reversed(ordre):
                    boot(cas,f"stockage-final-{n}")
    elif a.lot in ("arm","transports","capacite"):
        arm=("cycles-256","court","rattrapage","recu-arm-differe","recu-arm-absent")
        replis=("choix-data","plein")
        if a.lot=="arm":
            lancer("performance.py","compiler",port,*arm,*replis,"--revision",FINAL)
            lancer("performance.py","rejouer",port,*arm,"--arch","arm","--cycle-vie","--repetitions",1,
                   "--sortie",out/("arm-"+FINAL[:7]))
        elif a.lot=="capacite":
            lancer("performance.py","rejouer",port,"court","--arch","arm","--cycle-vie","--repetitions",1,
                   "--sortie",out/"capacite-arm")
        else:
            lancer("performance.py","rejouer",port,*replis,"--repetitions",1,"--sortie",out/("transport-"+FINAL[:7]))
    elif a.lot=="ethernet":
        images={"ancien3b":wt/"archives-ethernet-phase3b/images-ff864f5",
                "ancien3c":wt/"archives-ethernet-phase3c/images-0ba527c"}
        for v in ("temoin","froid"):
            images[v]=out/("images-echo-"+v)
            lancer("performance.py","compiler",images[v],"echo1514","--revision",BASE,
                   *(["--cloture-froide"] if v=="froid" else []))
        for n in range(1,6):
            for v in images if n%2 else reversed(images):
                lancer("performance.py","rejouer",images[v],"echo1514","--arch","q35","--sortie",out/("echo-"+v),
                       "--repetitions",n,*([] if v=="ancien3b" else ["--cycle-vie"]))
    else:
        lancer("hote_retrait.py",out/"hote")


if __name__=="__main__":
    main()
