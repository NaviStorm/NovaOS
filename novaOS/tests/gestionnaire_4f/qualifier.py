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


def verifier_lancements(serie, cycles, boot):
    lancements = re.findall(r"D4 lancement scelle epoch=\d+ pid=(\d+) incarnation=(\d+) generation=(\d+)", serie)
    recus = re.findall(r"E41 recu generation=(\d+) intention=\d+ pilote=(\d+) incarnation=(\d+) creations=1 rejeu=sans_effet", serie)
    if len(lancements) != cycles or [(g,p,i) for p,i,g in lancements] != recus:
        raise ValueError("Désaccord entre créations scellées et reçus")
    if re.findall(r"E41 autorite boot=([0-9a-f]{32})", serie) != [boot]*cycles:
        raise ValueError("Époque de l’autorité")
    coex = re.findall(r"D4 coexistence mode=\d+ racines=\[(\d+), (\d+), (\d+)\] boites=(\d+)", serie)
    if len(coex) != cycles or any(max(map(int, racines)) > 4 or int(boites) > 8 for *racines, boites in coex):
        raise ValueError("Plafonds IPC par acteur et globaux")
    if len(re.findall(r"D4 PT_LOAD compares=[1-9]\d* zeros=\d+ avant_capacites=true", serie)) != cycles:
        raise ValueError("Vérification du segment avant attribution")


def verifier_remplacements(cas, serie, changements):
    for tour, ligne in enumerate(changements, 1):
        champs = dict(re.findall(r"(\w+)=([\w-]+)", ligne))
        ancien, nouveau = int(champs["ancien_pid"]), int(champs["nouveau_pid"])
        if (ancien == nouveau or int(champs["ancien_inc"]) == int(champs["nouvel_inc"])
                or int(champs["ancienne_liaison"]) == int(champs["nouvelle_liaison"])
                or champs["ancien_refuse"] != "true" or int(champs["hz"]) == 0):
            raise ValueError(("Identité du remplaçant ou ancienne capacité", cas))
        if cas == "avant-selection":
            if not ligne.startswith("E41 reprise_selection "):
                raise ValueError("Reprise de la découverte initiale")
        else:
            phase = "attribution" if cas in {"concurrence", "endurance"} else cas
            if (champs["phase"] != phase or int(champs["role"]) != (2 if cas.startswith("lanceur") else 1)
                    or int(champs["generation"]) != (0 if cas == "apres-selection" else tour)):
                raise ValueError(("Rôle, génération ou point de reprise", cas))
        if cas == "concurrence":
            candidats = re.findall(r"E41 concurrence gagnant=(\d+) refuse=(\d+) sortie=Exit\(10\) vues_perdant=0", serie)
            if len(candidats) != 1 or int(candidats[0][0]) != nouveau or int(candidats[0][1]) == nouveau:
                raise ValueError("Isolation du candidat perdant")
    if cas in {"publication", "gestionnaire-actif", "lanceur-actif"}:
        publications = re.findall(r"^E41 publication .*", serie, re.M)
        if len(publications) != 1 or not re.fullmatch(
                r"E41 publication generation=1 sequence=\d+ resultat=1 remise=1", publications[0].strip()):
            raise ValueError("Publication incertaine ou remise répétée")


def controler(cas, dest, *, enregistrer=True):
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
    verifier_lancements(texte, cycles, next(iter(boots)))
    if len({r[2] for r in recus}) != cycles or len({r[3] for r in recus}) != cycles:
        raise ValueError("Titulaire ou incarnation réutilisé")
    if [int(r[0]) for r in recus] != list(range(1, cycles + 1)):
        raise ValueError("Génération du reçu")
    for i, fin in enumerate(fins, 1):
        if fin[0] != str(i):
            raise ValueError("Incarnation ou génération")
        attendu = ("4194303","4194303","0","true","false") if cas=="absent" else ("0","0",str(2*i+1),"false","true")
        if not gel_journal and fin[1:] != attendu:
            raise ValueError(("Disposition du bail",fin,attendu))
    changements = [s for s in texte.splitlines() if s.startswith(("E41 reprise ", "E41 reprise_selection "))]
    attendu = 0 if gel_journal or cas in {"nominal","differe","absent","ethernet"} else cycles
    if len(changements) != attendu:
        raise ValueError(("Remplacement attendu", cas, changements))
    verifier_remplacements(cas, texte, changements)
    ressources = re.findall(r"D4 cycle avant=(\d+) minimum=(\d+) apres=(\d+) pic=(\d+) ipc=(\d+) dma=(\d+)", texte)
    if len(ressources)!=cycles or any(int(r[4]) or int(r[5])!=(30 if cas=="absent" or gel_journal else 3) for r in ressources):
        raise ValueError("Comptes après cycle")
    if len({r[2] for r in ressources})!=1 and cycles>1:
        raise ValueError("Dérive des cadres libres")
    doc = dict(boot=boots.pop(), recus=recus, fins=fins, ressources=ressources, reprises=changements)
    if enregistrer:
        write(dest/"reprise.json", doc)
    return doc


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
