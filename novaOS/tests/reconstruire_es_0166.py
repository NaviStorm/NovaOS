"""Reconstruit dans des exports Git distincts les invités de qualification des E/S groupées."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tarfile

RACINE = Path(__file__).resolve().parents[2]
PROFILS = ("brut", "regression", "sans", "sonde", "coupe", "defauts", "course",
           "utm", "utm-sonde", "utm-coupe")
CIBLES = ("aarch64-unknown-none-softfloat", "x86_64-unknown-none")


def empreinte(fichier):
    with fichier.open("rb") as flux:
        return hashlib.file_digest(flux, "sha256").hexdigest()


def fixtures(profil):
    if profil == "brut":
        return []
    resultat = ["io-0166-prototype", "io-0166-plages"]
    sonde = profil in ("sonde", "utm-sonde")
    coupe = profil in ("coupe", "defauts", "utm-coupe")
    if sonde:
        resultat += ["io-0166-prototype-sonde", "io-0166-plages-sonde"]
    if coupe:
        resultat += ["io-0166-plages-coupure", "io-0166-barrieres"]
    if profil == "course":
        resultat += ["vi-conflit-source", "io-0166-course-nom"]
    if profil.startswith("utm"):
        resultat.append("io-0166-systeme-qualification")
    if profil != "regression":
        resultat.append("io-0166-charge-4m")
    resultat += ["io-0166-virtio-multipage", "io-0166-invite"]
    if sonde:
        resultat += ["io-0166-dma-sonde", "io-0166-vcpu", "io-0166-irq-latences",
                     "io-0166-negociation-sonde"]
    if coupe:
        resultat.append("io-0166-dma-coupure")
    if profil == "defauts":
        resultat.append("io-0166-dma-defauts")
    return resultat + ["io-0166-negociation"]


def preparer_sources(archive, dossier, profil):
    # Aucun ancien target/ ni fichier non suivi n’entre dans le profil.
    dossier.mkdir()
    with tarfile.open(archive) as t:
        t.extractall(dossier, filter="data")
    traces = {}
    for nom in fixtures(profil):
        fichier = dossier/"novaOS/tests/fixtures"/(nom+".patch")
        with fichier.open("rb") as f:
            subprocess.run(["patch", "-p1", "-F0", "--batch", "--forward", "-s"],
                           cwd=dossier, stdin=f, check=True)
        traces[nom] = empreinte(fichier)
    return traces


def empreinte_sources(dossier):
    cumul = hashlib.sha256()
    chemins = sorted(f for f in dossier.rglob("*") if f.is_file())
    for fichier in chemins:
        relatif = fichier.relative_to(dossier).as_posix().encode()
        cumul.update(relatif + b"\0" + bytes.fromhex(empreinte(fichier)))
    return dict(fichiers=len(chemins), sha256_chemins_et_contenus=cumul.hexdigest())


def construire(archive, revision, racine, profil, env):
    dossier = racine/profil
    fiche = dict(revision_git=revision, profil=profil,
                 fixtures=preparer_sources(archive, dossier, profil), commandes=[], binaires={})
    fiche["sources_apres_fixtures"] = empreinte_sources(dossier)
    cibles = CIBLES[:1] if profil.startswith("utm") else CIBLES
    with (racine/(profil+".compilation.log")).open("x") as log:
        def compiler(cmd, composant):
            fiche["commandes"].append(dict(repertoire=composant, commande=cmd))
            subprocess.run(cmd, cwd=dossier/composant, env=env, check=True,
                           stdout=log, stderr=subprocess.STDOUT)

        for cible in cibles:
            cmd = ["rustup","run","stable","cargo","build","--offline","--locked","--release","--target",cible]
            if profil.startswith("utm"):
                cmd += ["--features","systeme-gpt"]
            compiler(cmd, "novaOS")
            elf = Path("novaOS/target")/cible/"release/novaos"
            fiche["binaires"][str(elf)] = empreinte(dossier/elf)
        if profil == "regression":
            # Les deux témoins de firmware doivent aussi embarquer le noyau de ce candidat.
            for variante in ("gicv3-lab","dma-sequentiel"):
                cible = CIBLES[0]
                racine_cible = "target-"+variante
                compiler(["rustup","run","stable","cargo","build","--offline","--locked","--release",
                          "--target",cible,"--features",variante,"--target-dir",racine_cible], "novaOS")
                elf = Path("novaOS")/racine_cible/cible/"release/novaos"
                fiche["binaires"][str(elf)] = empreinte(dossier/elf)
        compiler(["./lab/run","build","--offline","--bin","novafs"],"novaFS")
        oracle = Path("novaFS/target/aarch64-apple-darwin/release/novafs")
        fiche["binaires"][str(oracle)] = empreinte(dossier/oracle)
        if profil == "sans":
            compiler(["./lab/run","build","--offline","--features","prototype-es-groupees",
                      "--example","es-groupees"], "novaFS")
            outil = Path("novaFS/target/aarch64-apple-darwin/release/examples/es-groupees")
            fiche["binaires"][str(outil)] = empreinte(dossier/outil)
    (dossier/"provenance.json").write_text(json.dumps(fiche,ensure_ascii=False,indent=2)+"\n")
    print(f"{profil} construit ({revision})", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("revision", help="commit Git contenant le candidat et ses fixtures")
    parser.add_argument("sortie", type=Path, help="chaque profil doit y être absent")
    parser.add_argument("profils", choices=PROFILS, nargs="+")
    parser.add_argument("--jobs", type=int, choices=range(1, 9), default=2)
    args = parser.parse_args()
    out = args.sortie.resolve()
    out.mkdir(parents=True, exist_ok=True)
    if len(set(args.profils)) != len(args.profils) or any((out/p).exists() for p in args.profils):
        parser.error("Chaque profil doit être unique et son dossier neuf")
    revision = subprocess.check_output(["git","rev-parse","--verify",args.revision+"^{commit}"],
                                      cwd=RACINE,text=True).strip()
    # Les lots lancés séparément conservent leur propre archive et leur provenance.
    archive = out/("source-"+revision+"-"+args.profils[0]+".tar")
    with archive.open("xb") as f:
        subprocess.run(["git","archive",revision,"novaOS","novaFS","components",
                        ":(exclude)novaOS/docs/**",":(exclude)novaFS/docs/**"],
                       cwd=RACINE, stdout=f, check=True)
    rustc = subprocess.check_output(["rustup","which","--toolchain","stable","rustc"],text=True).strip()
    env = dict(os.environ,RUSTC=rustc,CARGO_NET_OFFLINE="true",NOVAFS_TARGET="aarch64-apple-darwin")
    env.pop("CARGO_TARGET_DIR",None)
    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
        futurs = [pool.submit(construire,archive,revision,out,p,env) for p in args.profils]
        for f in futurs:
            f.result()


if __name__ == "__main__":
    main()
