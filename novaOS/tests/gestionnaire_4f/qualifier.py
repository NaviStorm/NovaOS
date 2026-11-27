"""Qualification du profil dynamique Q35 simple. Le lanceur historique possède le verrou QEMU."""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

NOVA = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(NOVA / "tests"))
from gestionnaire_4d.qualifier import BASE, PORT, elf, output, sha, write
from ethernet_v1.cycle_vie import nombre_cycles

CAS = {key: "court" for key in (
    "nominal", "avant-selection", "apres-selection", "reservation", "lanceur-avant",
    "lanceur-engage", "lanceur-apres", "attribution", "deces-observe", "lanceur-observe",
    "gestionnaire-actif", "lanceur-actif", "publication", "reconciliation", "concurrence",
    "journal-perdu", "non-corroborable",
)}
CAS.update(endurance="cycles-256-court", differe="recu-vtd-differe", absent="recu-vtd-absent",
           ethernet="echo1514", statique="echo1514")
ISA = "x86_64-unknown-none"


def construire(cas, archive, revision=None):
    rev = output("git", "rev-parse", "--verify", (revision or "HEAD") + "^{commit}")
    dest = archive / "images" / cas
    exe = dest / "q35.elf"
    if exe.exists():
        doc = json.loads((dest/"construction.json").read_text())
        if doc["commit"] != rev or sha(exe) != doc["elf"]["q35"]:
            raise ValueError("Révision ou empreinte ELF divergente")
        return exe, rev
    if rev != output("git", "rev-parse", "HEAD"):
        raise ValueError("Image absente pour cette révision archivée")
    if output("git", "status", "--porcelain", "--", "novaOS/kernel", "novaOS/user", "novaOS/libs"):
        raise ValueError("Sources de construction modifiées")
    target = Path(os.environ["CARGO_TARGET_DIR"]).resolve()
    if Path(os.environ["NOVAOS_TMPDIR"]).resolve() not in target.parents:
        raise ValueError("Répertoire de compilation privé requis")
    dest.mkdir(parents=True, exist_ok=True)
    feature = "net-q35-lab,ethernet-v1-port" if cas == "statique" else "gestionnaire-4f"
    env = dict(os.environ, RUSTC=output("rustup", "which", "--toolchain", "stable", "rustc"),
               CARGO_PROFILE_RELEASE_STRIP="none", NOVAOS_ETH3_CASE=CAS[cas],
               NOVAOS_4D_REFUS="", NOVAOS_4E1_CAS=cas)
    cmd = ["rustup", "run", "stable", "cargo", "build", "--locked", "--offline", "--release",
           "--features", feature, "--target", ISA]
    with (dest/"construction.log").open("w") as log:
        subprocess.run(cmd, cwd=NOVA, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
    source = target/ISA/"release"
    shutil.copyfile(source/"novaos", exe)
    doc = dict(commit=rev, elf={"q35":sha(exe)}, noyau=elf(exe), commande=cmd,
               rustc=output(env["RUSTC"], "-V"), scenario=cas, cas=CAS[cas])
    if cas != "statique":
        out, = [p.parent for p in source.glob("build/novaos-*/out/gestionnaire.elf")]
        doc["acteurs"] = {name:elf(out/name) for name in ("gestionnaire.elf", "pair.elf", "net_preparation.bin", "ethernet_port.bin", "ethernet_client.bin")}
        payload = (out/"net_preparation.bin").read_bytes()
        package = (out/"pilote.pkg01").read_bytes()
        if package[int.from_bytes(package[:4],"little")+68:] != payload or payload[18:20] != b"\x3e\x00":
            raise ValueError("ELF du paquet")
        doc["paquet"] = sha(out/"pilote.pkg01")
        doc["manifeste"] = sha(out/"pilote.toml")
    write(dest/"construction.json",doc)
    return exe, rev


def controler(cas, dest):
    c = json.loads((dest/"campagne.json").read_text())
    if not c["succes"] or c["erreur_pair"] or c["entrees"]["novaos.elf"] != c["elf"]:
        raise ValueError("Empreinte Limine, pair ou oracles matériels")
    if cas == "statique":
        return
    texte = (dest/"serie.log").read_text()
    cycles = nombre_cycles(CAS[cas], "q35")
    boots = set(re.findall(r"E41 demarrage boot=([0-9a-f]{32})", texte))
    recus = re.findall(r"E41 recu generation=(\d+) intention=(\d+) pilote=(\d+) incarnation=(\d+) creations=1 rejeu=sans_effet", texte)
    fins = re.findall(r"E41 fin generation=(\d+) creations=1 epingles=(\d+) mappes=(\d+) recu=(\d+) gel=(\w+) compactage=(\w+)", texte)
    gel_journal = cas in {"journal-perdu", "non-corroborable"}
    if gel_journal:
        preuve = rf"E41 gel cause={cas} generation=1 recu_observe=3 traductions=0 epingles_proprietaire=22"
        if preuve not in texte or "compactage=false" not in texte or len(recus) != 1:
            raise ValueError("Perte du journal et conservation physique")
        if cas == "journal-perdu" and "E41 fin generation=0 creations=0" not in texte:
            raise ValueError("Ancienne vue encore publiée")
    if len(boots) != 1 or len(recus) != cycles or (not gel_journal and len(fins) != cycles):
        raise ValueError("Époque ou unicité du lancement")
    for i, (r, fin) in enumerate(zip(recus, fins), 1):
        if int(r[0]) != i or fin[0] != str(i) or len(set(x[2] for x in recus)) != cycles:
            raise ValueError("Incarnation ou génération")
        attendu = ("4194303","4194303","0","true","false") if cas=="absent" else ("0","0",str(2*i+1),"false","true")
        if not gel_journal and fin[1:] != attendu:
            raise ValueError(("Disposition du bail",fin,attendu))
    changements = [s for s in texte.splitlines() if s.startswith(("E41 reprise ", "E41 reprise_selection "))]
    if not gel_journal and cas in set(CAS)-{"nominal","endurance","differe","absent","ethernet","statique"} and len(changements)!=cycles:
        raise ValueError(("Remplacement attendu", cas, changements))
    ressources = re.findall(r"D4 cycle avant=(\d+) minimum=(\d+) apres=(\d+) pic=(\d+) ipc=(\d+) dma=(\d+)", texte)
    if len(ressources)!=cycles or any(int(r[4]) or int(r[5])!=(30 if cas=="absent" or gel_journal else 3) for r in ressources):
        raise ValueError("Comptes après cycle")
    if len({r[2] for r in ressources})!=1 and cycles>1:
        raise ValueError("Dérive des cadres libres")
    write(dest/"reprise.json",dict(boot=boots.pop(), recus=recus, fins=fins, ressources=ressources, reprises=changements))


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("archive", type=lambda s:Path(s).resolve())
    p.add_argument("cas", choices=CAS)
    p.add_argument("--boots", type=int, default=3, choices=(1,2,3))
    p.add_argument("--revision", help="Rejouer une image archivée de cette révision")
    p.add_argument("--compiler", action="store_true")
    a = p.parse_args()
    exe,rev = construire(a.cas,a.archive,a.revision)
    if a.compiler: return
    outils = BASE/"archives-ethernet-phase3/reproduction-8bd3f9c"
    for n in range(1,a.boots+1):
        dest = a.archive/"simple"/a.cas/str(n)
        if not (dest/"campagne.json").exists():
            cmd = [sys.executable,"-B",PORT,"q35",CAS[a.cas],exe,dest,BASE/"pilote-virtio-net",
                   BASE/"archives-virtio-net", outils/"limine-enroll", outils/"BOOTAA64.EFI",
                   "--source-elf",rev,"--cycle-vie","--delai","1200" if a.cas=="endurance" else "300"]
            if a.cas!="statique":
                cmd.extend(["--gestionnaire-4f",a.cas])
            subprocess.run(cmd,check=True)
        controler(a.cas,dest)
        trace=dest/"qemu.trace"
        if trace.exists():
            with trace.open("rb") as src, gzip.open(dest/"qemu.trace.gz","xb",compresslevel=1) as dst:
                shutil.copyfileobj(src,dst)
            trace.unlink()
    if a.boots>1 and a.cas!="statique":
        epochs = {json.loads((a.archive/"simple"/a.cas/str(n)/"reprise.json").read_text())["boot"] for n in range(1,a.boots+1)}
        if len(epochs)!=a.boots: raise ValueError("Démarrages distincts attendus")


if __name__ == "__main__":
    main()
