"""Qualification hors ligne du disque P9 sous le DXE AArch64 avec ou sans domaine réseau."""
if not __debug__:
    raise RuntimeError("Les contrôles de qualification exigent les assertions Python.")

import argparse
import ast
import json
import math
import os
from pathlib import Path
import re
import statistics
import subprocess
import sys
import tempfile
import time
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from es_groupees_utm import image
from fichiers import PATTERN, HOST, fixture, oracle
from installation_utm import lanceurs
from prototype_uefi import tpm
from verifier_gpt import verifier_gpt
from virtio_net_confinement import ROOT, digest, dump
from virtio_net_dxe import firmware_files, observe
from virtio_net_qemu import qualification


def initial(path, secteur):
    ancien = path / "initial.p8"
    fixture(SimpleNamespace(image=ancien, sector=secteur), existing=True, blocks=512)
    with ancien.open("rb") as fp:
        inspect = subprocess.check_output([HOST, "inspect", "512", str(secteur)], stdin=fp)
    identites = set(re.findall(rb"FS=(\w+) LV=(\w+)", inspect))
    if len(identites) != 1:
        raise ValueError("Identités de la fixture incohérentes")
    fs, lv = (s.decode() for s in identites.pop())
    p9 = path / "initial.p9"
    receipt = subprocess.check_output([sys.executable, ROOT.parent / "novaFS/lab/migrate-p9.py",
            ancien, p9, "--outil", HOST, "--secteur", str(secteur), "--profil", "contenu", "--fs", fs, "--lv", lv])
    (path / "migration.json").write_bytes(receipt)
    return p9, bytes.fromhex(lv)


def mesures(serial, prefix, count):
    match = re.search(r"NET ARM " + prefix + r"(?: secteur=\d+)? hz=(\d+)(?: octets=4096)? cycles=(\[[0-9, ]+\])", serial)
    if not match:
        raise ValueError("Échantillons manquants pour " + prefix)
    hz = int(match[1])
    cycles = ast.literal_eval(match[2])
    if len(cycles) != count or min(cycles) <= 0 or hz <= 0:
        raise ValueError("Géométrie temporelle invalide")
    us = sorted(c * 1e6 / hz for c in cycles)
    result = dict(n=count, hz=hz, mediane_us=statistics.median(us), p95_us=us[math.ceil(count * .95)-1],
                  p99_us=us[math.ceil(count * .99)-1], moyenne_us=statistics.mean(us))
    if prefix == "lectures":
        result["Mio_s"] = count * 4096 * hz / sum(cycles) / (1 << 20)
    return result


def run(elf, source, secteur, dossier, installer=False, fichiers=False, event_idx=None, ethernet=False):
    dossier.mkdir(parents=True, exist_ok=False)
    report = {"elf_sha256": digest(elf), "trafic_demande": ethernet, "bail_nic": False, "secteur": secteur}
    try:
        src = source / str(secteur)
        if not src.exists():
            src.mkdir(parents=True)
            initial(src, secteur)
        vol = src / "initial.p9"
        migration = json.loads((src / "migration.json").read_text())
        report["volume_sha256"] = digest(vol)
        if report["volume_sha256"] != migration["sha256"]:
            raise ValueError("Volume initial modifié")
        with vol.open("rb") as fp:
            observed = subprocess.check_output([HOST, "inspect", "512", str(secteur)], stdin=fp)
        ids = set(re.findall(rb" LV=(\w+)", observed))
        if len(ids) != 1:
            raise ValueError("Identité P9 ambiguë")
        lv = bytes.fromhex(ids.pop().decode())
        disque = dossier / "systeme.raw"
        gpt = image(disque, vol, elf, secteur, scenario="net-arm-p9", volume_id=lv)
        report["gpt_initial_sha256"] = digest(disque)
        with tempfile.TemporaryDirectory(dir=os.environ["NOVAOS_TMPDIR"], prefix="net") as td:
            tmp = Path(td)
            lanceurs(tmp)
            report["distribution_sha256"] = firmware_files(tmp)
            with qualification(ROOT.parent.parent, report):
                start = time.monotonic()
                try:
                    with tpm(tmp, dossier, "labo") as sock:
                        report["bilan_invite"] = observe(tmp, dossier, sock, disque=disque, secteur=secteur, phase=("ethernet-p9" if os.environ.get("NOVAOS_NET_PAIR_P9")=="lectures-flush" else "ethernet-prive") if ethernet else "files-nic" if installer else "parcours-fichiers" if fichiers else None, event_idx=event_idx)
                finally:
                    report["execution_s"] = time.monotonic() - start
        if not report["bilan_invite"]:
            raise RuntimeError("Le noyau de qualification n'a pas confirmé son parcours")
        map_gpt = verifier_gpt(disque, secteur, gpt)
        extrait = dossier / "volume-relu.p9"
        with disque.open("rb") as fp, extrait.open("xb") as dst:
            fp.seek(map_gpt["p9"] * secteur)
            dst.write(fp.read(vol.stat().st_size))
        if ethernet:
            from virtio_net_domaines_arm import verifier
            report["domaines"]=verifier(dossier, True, ethernet=True)
            serial=(dossier/"serie.log").read_text()
            pair=json.loads((dossier/"pair.json").read_text())
            if not (pair["succes"] and digest(extrait)==digest(vol) and
                    "NET ARM L2 retrait disque_lisible=true cadres_fin=0" in serial and
                    "NET L2 pilote fin=Exit(0) racines=0" in serial and
                    "NET ARM L2 INTID=38 isr=0x101" in serial and
                    "NET L2 completions TX=16 RX=16 identifiants_uniques=true" in serial):
                raise ValueError("Ethernet, IRQ, ressources ou contenu P9 non confirmés")
            if os.environ.get("NOVAOS_NET_PAIR_P9")=="lectures-flush":
                report["lectures"]=mesures(serial,"lectures",64)
                report["flush"]=mesures(serial,"flush",16)
                if not pair.get("stockage_avant_derniere_reponse"):
                    raise ValueError("Chevauchement stockage/réseau non établi")
            total=pair.get("lots",1)*16
            if f"NET L2 total TX={total} RX={total} index={total%65536} rearmements={total//16-1}" not in serial:
                raise ValueError("Indices terminaux ou réarmements incomplets")
            cycles=pair.get("cycles",1)
            if cycles>1:
                from virtio_net_pair import verifier_redemarrages
                verifier_redemarrages(serial,pair)
            report.update(succes=True, bail_nic=True, trames_par_sens=total*cycles)
            return
        if event_idx is not None:
            from virtio_net_event_idx import verifier_negociation
            from virtio_net_domaines_arm import verifier as domaines
            report["negociation"] = verifier_negociation(dossier, event_idx, "ARM")
            report["domaines"] = domaines(dossier, event_idx == "on")
            if event_idx == "off":
                serial = (dossier / "serie.log").read_text()
                if "NET ARM refus preparation=Feature cadres_dma=0" not in serial or digest(extrait) != digest(vol):
                    raise ValueError("Refus sans restitution ou volume initial différent")
                report["succes"] = True
                return
        attendu = bytearray(PATTERN)
        if fichiers:
            attendu[4091:4106] = b"NovaFS-partiel!"
        report["generation"] = oracle(SimpleNamespace(image=extrait, sector=secteur), bytes(attendu))
        report["p9_relu_sha256"] = digest(extrait)
        serial = (dossier / "serie.log").read_text(errors="replace")
        report["lectures"] = mesures(serial, "lectures", 64)
        report["flush"] = mesures(serial, "flush", 16)
        report["succes"] = True
    finally:
        dump(dossier / "campagne.json", report)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("elf", type=Path)
    p.add_argument("sources", type=Path)
    p.add_argument("secteur", type=int, choices=(512, 4096))
    p.add_argument("sortie", type=Path)
    profils = p.add_mutually_exclusive_group()
    profils.add_argument("--installer-files", action="store_true")
    profils.add_argument("--parcours-fichiers", action="store_true")
    profils.add_argument("--ethernet", action="store_true")
    p.add_argument("--event-idx", choices=("on", "off"))
    a = p.parse_args()
    if a.ethernet and (a.event_idx!="on" or not os.environ.get("NOVAOS_QEMU_RX_ARM")):
        p.error("Le parcours Ethernet exige le modèle corrigé et EVENT_IDX")
    if a.event_idx and not (a.installer_files or a.ethernet):
        p.error("--event-idx exige --installer-files")
    run(a.elf.resolve(), a.sources.resolve(), a.secteur, a.sortie.resolve(), a.installer_files, a.parcours_fichiers, a.event_idx, a.ethernet)
