#!/usr/bin/env python3
"""Comparer P7 et P7.1 sur copies de sources et images jetables, sans réécrire le checkout."""
import argparse
from collections import defaultdict
import gzip
import hashlib
import json
import os
from pathlib import Path
import platform
import runpy
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
OLD = "6ed50784ae73666caa15846ff380e5c7824faa36"
P7 = runpy.run_path(str(ROOT / "lab/p7-validation.py"))
ARCHES = P7["ARCHES"]
PROFILS = (("p7-clair", "none", "aleatoire"), ("p7-chiffre", "none", "aleatoire"),
           ("p7-chiffre", "lz4", "repetitif"), ("p7-chiffre", "lz4", "aleatoire"))


def inserer(fichier, avant, apres):
    s = fichier.read_text()
    if s.count(avant) != 1:
        raise ValueError(f"point de mesure absent ou ambigu dans {fichier}: {avant}")
    fichier.write_text(s.replace(avant, apres))


def sonde(nom):
    return ('#[cfg(feature = "latency-probes")]\n'
            f'        let _sonde = crate::latency_probes::enter(crate::latency_probes::Phase::{nom});\n        ')


def instrumenter(racine):
    # Les mêmes ajouts sont appliqués aux deux copies avant compilation. Aucun
    # remplacement des algorithmes, du format ou de l'ordre des E/S du moteur.
    phases = racine / "lab/latency_probes.rs"
    inserer(phases, '    Decompression,\n}', '    Decompression,\n    Graph,\n    UnitAead,\n}')
    inserer(phases, '[&str; 12]', '[&str; 14]')
    inserer(phases, '    "decompression_lz4",', '    "decompression_lz4",\n    "graphe",\n    "aead_unite",')
    graphe = racine / "src/shared_graph.rs"
    debut = 'pub(crate) fn open(dev: &impl Device) -> Result<Self> {'
    inserer(graphe, debut, debut + '\n        ' + sonde('Graph'))
    debut = '        let mut graph = Self::index_roots(dev, head, previous);'
    inserer(graphe, debut, '        ' + sonde('Graph') + debut.lstrip())
    inserer(graphe, '        inspect(&graph, &e)', '        #[cfg(feature = "latency-probes")]\n        drop(_sonde);\n        inspect(&graph, &e)')
    unite = racine / 'src/secure_unit.rs'
    for appel in ['let (nonce, tag) = keys.key(ctx)?.seal(', 'keys.key(ctx)?.open(']:
        inserer(unite, appel, sonde('UnitAead') + appel)
    banc = racine / 'examples/p7-lab.rs'
    inserer(banc, '    writes: u64,', '    reads: u64,\n    writes: u64,')
    inserer(banc, '        self.0.read(b)', '''        let mut c = self.1.get();
        c.reads += 1;
        self.1.set(c);
        self.0.read(b)''')
    inserer(banc, '        let old = self.io.get();', '''        let old = self.io.get();
        #[cfg(feature = "latency-probes")]
        novafs::latency_probes::begin();
        let allocations = heap_meter::ALLOCATIONS.load(Relaxed);''')
    inserer(banc, '        let after = self.io.get();', '''        let after = self.io.get();
        let allocations = heap_meter::ALLOCATIONS.load(Relaxed) - allocations;
        #[cfg(feature = "latency-probes")]
        let phases = novafs::latency_probes::finish();
        #[cfg(not(feature = "latency-probes"))]
        let phases = serde_json::Value::Null;''')
    inserer(banc, '"op":op,"octets"', '"op":op,"lectures_physiques":after.reads-old.reads,"allocations":allocations,"phases":phases,"octets"')


def construire(source, isa, sondes, dest):
    cmd = [source/"lab/run", "build", "--offline", "--example", "p7-lab", "--message-format=json"]
    if sondes:
        cmd.extend(["--features", "latency-probes"])
    env = dict(os.environ, NOVAFS_TARGET=isa+"-apple-darwin", CARGO_TARGET_DIR=str(source.parent/"build"))
    lignes = subprocess.check_output(cmd, env=env, text=True)
    exe, = [Path(m["executable"]) for l in lignes.splitlines() if (m := json.loads(l)).get("executable")]
    assert exe.read_bytes()[:8] == bytes.fromhex("cffaedfe") + P7["CPUS"][isa].to_bytes(4, "little")
    return Path(shutil.copy2(exe, dest))


def echanges(binaires, tmp):
    resultats = []
    for isa, lecteur in (ARCHES, ARCHES[::-1]):
        for version, autre in (("p7", "p71"), ("p71", "p7")):
            for secteur in (512, 4096):
                with tempfile.TemporaryDirectory(dir=tmp) as d:
                    d = Path(d)
                    P7["run"](binaires[version, isa, False], "produire", secteur, d)
                    image = d/"pool"
                    digest = hashlib.sha256(image.read_bytes()).hexdigest()
                    exe = binaires[autre, lecteur, False]
                    source, = P7["run"](exe, "lire", secteur, d)
                    assert hashlib.sha256(image.read_bytes()).hexdigest() == digest
                    image.unlink()
                    restauration, = P7["run"](exe, "restaurer", 4096 if secteur == 512 else 512, d)
                    assert restauration["domaine_restaure"] != source["domaine_source"]
                    resultats.append(dict(producteur=version, lecteur=autre, isa=isa, isa_lecteur=lecteur,
                                           secteur=secteur, image_sha256=digest))
    return resultats


def mesurer(binaires, sortie):
    groupes = defaultdict(list)
    processus = []
    with gzip.open(sortie/"appels.jsonl.gz", "wt") as brut:
        for isa in ARCHES:
            for sondes in (False, True):
                profils = PROFILS if not sondes else (PROFILS[2],)
                for rep in range(2):
                    versions = ("p7", "p71") if rep == 0 else ("p71", "p7")
                    for secteur in (512, 4096):
                        for fmt, codec, donnees in profils:
                            for v in versions:
                                evenements, proc = P7["run_measured"](binaires[v, isa, sondes], "mesure",
                                    secteur, fmt, codec, donnees, 0x1357+rep)
                                contexte = dict(version=v, isa=isa, sondes=sondes, secteur=secteur,
                                    format=fmt, codec=codec, donnees=donnees, repetition=rep)
                                processus.append(dict(**contexte, **proc))
                                assert sum(e["op"]=="ecriture_clone" for e in evenements)==32
                                assert sum(e["op"]=="lecture_clone" for e in evenements)==32
                                for e in evenements:
                                    if e["op"] in ("ecriture_clone","lecture_clone"):
                                        assert e["flushs"] == (2 if e["op"]=="ecriture_clone" else 0)
                                    groupes[v, isa, sondes, fmt, codec, donnees, e["op"]].append(e)
                                    brut.write(json.dumps(dict(**contexte, **e), separators=(",",":"))+"\n")
                    print(isa, "sondes" if sondes else "latences", rep+1, flush=True)
    bilan = []
    for (v, isa, sondes, fmt, codec, donnees, op), e in groupes.items():
        ligne = dict(version=v, isa=isa, sondes=sondes, format=fmt, codec=codec, donnees=donnees,
                     op=op, **P7["summarize"](e))
        for champ in ("lectures_physiques", "allocations"):
            ligne[champ] = sum(x[champ] for x in e)
        if sondes:
            for phase in ("graphe", "aead_unite"):
                ligne[phase] = {k:sum(x["phases"][phase][dim][c] for x in e)
                    for k, dim, c in (("nombre","mur","n"),("mur_ns","mur","somme_ns"),
                                      ("cpu_ns","cpu_thread","somme_ns"))}
        bilan.append(ligne)
    return dict(processus=processus, distributions=bilan)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--sortie", required=True, type=Path)
    p.add_argument("--etape", choices=("tout","preparer","mesurer"), default="tout")
    opts = p.parse_args()
    sortie = opts.sortie.resolve()
    repertoire = sortie/"binaires"
    repertoire.mkdir(parents=True, exist_ok=True)
    binaires = {(v,isa,sondes):repertoire/f"{v}-{isa}-{sondes}"
                for v in ("p7","p71") for isa in ARCHES for sondes in (False,True)}
    if opts.etape != "mesurer":
        with tempfile.TemporaryDirectory(prefix="novafs-p71-") as nom:
            tmp = Path(nom)
            archive = tmp/"p7.tar"
            with archive.open("wb") as file:
                subprocess.run(["git", "archive", OLD, "novaFS"], cwd=ROOT.parent, stdout=file, check=True)
            ancien = tmp/"p7"
            ancien.mkdir()
            subprocess.run(["tar","-xf",archive,"-C",ancien],check=True)
            courant = tmp/"p71/novaFS"
            shutil.copytree(ROOT, courant, ignore=shutil.ignore_patterns("target","docs","__pycache__"))
            for v,source in (("p7", ancien/"novaFS"), ("p71",courant)):
                instrumenter(source)
                for isa in ARCHES:
                    for sondes in (False,True):
                        construire(source, isa, sondes, binaires[v,isa,sondes])
            rapports = dict(reference=OLD, macos=platform.mac_ver()[0], rust=subprocess.check_output(
                ["rustup","run","stable","rustc","--version"],text=True).strip(),
                binaires={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in binaires.values()},
                echanges=echanges(binaires,tmp))
            P7["write"](sortie/"binaires.json",rapports)
    if opts.etape != "preparer":
        rapports = json.loads((sortie/"binaires.json").read_text())
        for exe in binaires.values():
            assert hashlib.sha256(exe.read_bytes()).hexdigest() == rapports["binaires"][exe.name]
        rapports["horloges"] = {isa:P7["run"](binaires["p71",isa,False],"horloge")[0] for isa in ARCHES}
        rapports.update(mesurer(binaires,sortie))
        P7["write"](sortie/"comparaison.json", rapports)


if __name__ == "__main__":
    main()
