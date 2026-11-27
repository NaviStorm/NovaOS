"""Vérifie une transaction P9 sur NVMe pendant le service Ethernet v1 sur Q35 PIO."""
import argparse
from contextlib import ExitStack
import json
import os
from pathlib import Path
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import time

import retrait_duo
from port_qemu import AUDIT, Pair, digest, dump, qualification


def verifier(chemin, cas, lecteur, avant, pair):
    from nvme_phase2 import application_order
    from nvme_phase2_oracle import chronology, inspect, io_trace, verify_transition
    serie = (chemin / "serie.log").read_text()
    trace = (chemin / "qemu.trace").read_text().splitlines()
    ops = io_trace(chemin / "qemu.trace", 512)
    apres = inspect(lecteur, chemin / "namespace.raw", 512)
    verify_transition(avant, apres, {"nouveau"})
    ordre = application_order(chronology(ops, avant, 512), serie)
    if "NVME bilan ok=true" not in serie or any(o["cqe"] != 0 for o in ops):
        raise ValueError("Bilan NVMe ou CQE")
    racines = {(int(p), int(n)) for p, n in re.findall(r"DUO IPC pid=(\d+) racines=(\d+)", serie)}
    if len(racines) != 7 or max(n for _, n in racines) > 4:
        raise ValueError(f"Racines des sept acteurs {racines}")
    if "racines=[0, 4, 1]" not in serie or any("vtd_dmar_fault " in s for s in trace):
        raise ValueError("Isolation DMA ou IPC")
    sid_sequence = [int(x) for x in re.findall(r"DUO WAIT sid=(\d+)", serie)]
    panne = cas == "deces"
    if sid_sequence != ([48, 56, 48] if panne else [48, 56, 56, 48]):
        raise ValueError(f"Ordre des invalidations {sid_sequence}")
    edges = {48: [], 56: []}
    for i, row in enumerate(trace):
        m = re.fullmatch(r"pci_cfg_write \S+ 00:0([67])\.0 @0x4 <- (0x[0-9a-f]+)", row)
        if m:
            sid = int(m[1]) * 8
            if bool(len(edges[sid]) % 2) != bool(int(m[2], 16) & 4):
                edges[sid].append(i)
    if any(len(e) != 2 for e in edges.values()):
        raise ValueError(f"Transitions BME inattendues {edges}")
    io = [i for i, row in enumerate(trace) if row.startswith("pci_nvme_io_cmd ")]
    apres_retrait = sum(edges[56][1] < i < edges[48][1] for i in io)
    commun = max(edges[48][0], edges[56][0]) < min(edges[48][1], edges[56][1])
    if not commun or pair.error:
        raise ValueError("Baux sans recouvrement ou trames divergentes")
    if panne:
        if (not apres_retrait or pair.frames != 4 or
            not re.search(r"DUO panne NIC pid=\d+ generation=1 BME=0 DMA_retenu=26 NVMe_verifie=true",serie) or
            "acteurs=[Exit(9), Exit(0), Exit(0)]" not in serie):
            raise ValueError("Continuité NVMe après la mort du pilote")
    elif pair.frames != 144 or "materiel=Ok(()) retrait=Ok(0) acteurs=[Exit(0), Exit(0), Exit(0)]" not in serie:
        raise ValueError("Drainage Ethernet incomplet")
    return dict(p9=apres, ordre=ordre, operations=len(ops), io_apres_retrait_nic=apres_retrait,
                racines=sorted(racines), bme=edges, recus_sid=sid_sequence, recouvrement=commun)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("cas", choices=("port", "deces", *retrait_duo.CAS))
    for nom in ("elf", "sortie", "temoin", "archive", "limine", "lecteur"):
        p.add_argument(nom, type=lambda s: Path(s).resolve())
    p.add_argument("--instantane", action="store_true", help="Relire sous QTest le domaine NVMe pendant la conservation des 26 cadres NIC")
    a = p.parse_args()
    panne_qi = {"duo-wait-absent": "absent", "duo-wait-tardif": "tardif"}.get(a.cas)
    if a.instantane and a.cas not in ("duo-differe", "duo-absent"):
        p.error("L’instantané nécessite un reçu de retrait en attente de livraison")
    if subprocess.check_output(["git", "-C", a.temoin, "rev-parse", "HEAD"], text=True).strip() != AUDIT:
        raise ValueError("Source du profil")
    sys.path[:0] = [str(a.temoin / "novaOS/tests"), str(a.temoin / "novaOS/scripts")]
    from bloc import qmp
    from nvme_phase2_oracle import inspect
    from virtio_net_confinement import capture
    from virtio_net_duo_pannes import tables_finales
    from virtio_net_pio_q35 import pci_etat, support_pio
    from virtio_net_relais_q35 import support
    import virtio_net_pair
    out = a.sortie
    out.mkdir(parents=True, exist_ok=False)
    ref = a.archive / "cloture-20261126/panne-nic-512-12"
    seed = a.archive / "finale-20261125/q35-duo-nvme-init-512-S01/qemu/namespace.raw"
    firmware = a.archive / "finale-20261125/q35-pio-firmware-02"
    if panne_qi:
        cloture = a.archive / "cloture-20261126"
        ref = cloture / f"attente-{panne_qi}-nic-512-12"
        os.environ.update(NOVAOS_Q35_WAIT=panne_qi, NOVAOS_Q35_PREUVES_WAIT=str(cloture),
                          NOVAOS_QEMU_RX_Q35=str(cloture / "modele-wait/compilation/11.1.3-1/construction.json"))
    args = json.loads((ref / "arguments.json").read_text())
    mode = retrait_duo.CAS.get(a.cas, "nvme-panne-nic" if a.cas == "deces" else "nvme-charge")
    rapport = dict(succes=False, cas=a.cas, audit=AUDIT, modele=digest(Path(args[0])),
                   elf=digest(a.elf), graine=digest(seed), lecteur=digest(a.lecteur))
    construction = json.loads((a.elf.parent / "sources.json").read_text())
    if construction["audit"] != AUDIT or construction["elf"].get(a.cas) != rapport["elf"]:
        raise ValueError("ELF différent du manifeste de construction")
    rapport["commit_sources"] = construction["port"]
    rapport["revision_banc"] = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    shutil.copyfile(a.elf.parent / "sources.json", out / "sources.json")
    rapport["sources_suivies_modifiees"] = bool(subprocess.check_output(["git", "status", "--porcelain", "--untracked-files=no"]))
    shutil.copyfile(a.elf, out / "novaos.elf")
    shutil.copyfile(seed, out / "namespace.raw")
    avant = inspect(a.lecteur, out / "namespace.raw", 512)
    if avant["etat"] != "ancien":
        raise ValueError("Graine P9")
    try:
        with ExitStack() as stack:
            tmpbase = Path(os.environ["NOVAOS_TMPDIR"])
            stack.enter_context(qualification(tmpbase.parent, rapport))
            tmp = Path(stack.enter_context(tempfile.TemporaryDirectory(dir=tmpbase, prefix="e3duo")))
            support(a.elf, firmware, tmp, a.limine, duo=mode)
            rapport["entrees"] = json.loads((tmp / "entrees.json").read_text())
            if panne_qi:
                from virtio_net_duo_attente import verifier_preconditions
                verifier_preconditions(firmware, tmp, mode)
                if digest(Path(args[0])) != json.loads(Path(os.environ["NOVAOS_QEMU_RX_Q35"]).read_text())["sha256"]:
                    raise ValueError("Modèle QEMU de WAIT différent du manifeste vérifié")
                rapport["faute_qi"] = panne_qi
            support_pio(tmp / "support.raw", out / "boot.fat")
            h, g = socket.socketpair()
            stack.callback(h.close)
            stack.callback(g.close)
            for i, v in enumerate(args):
                if v == "-netdev":
                    args[i+1] = re.sub(r"fd=\d+", f"fd={g.fileno()}", args[i+1])
                elif v in ("-serial", "-debugcon"):
                    args[i+1] = "file:" + str(out / Path(args[i+1]).name)
                elif v == "-drive" and "unit=1," in args[i+1]:
                    args[i+1] = re.sub(r"file=[^,]+", f"file={tmp}/variables.fd", args[i+1])
                elif v == "-drive" and "id=stockage," in args[i+1]:
                    args[i+1] = re.sub(r"file=[^,]+", f"file={out}/namespace.raw", args[i+1])
                elif v == "-fw_cfg" and "opt/novaos/boot.fat" in args[i+1]:
                    args[i+1] = f"name=opt/novaos/boot.fat,file={out}/boot.fat"
            dump(out / "arguments.json", args)
            pair = Pair(h, out, virtio_net_pair, "duo-reouverture" if a.cas == "duo-reouverture" else "cycles" if a.cas == "duo-cycles" else "deces" if mode == "nvme-panne-nic" else "port")
            with (out/"qemu.trace").open("xb") as log:
                with subprocess.Popen(args, cwd=tmp, pass_fds=(g.fileno(),), stdin=subprocess.PIPE,
                                      stdout=subprocess.PIPE, stderr=log, bufsize=0) as vm:
                    g.close()
                    pair.thread.start()
                    try:
                        qmp(vm, "qmp_capabilities")
                        rapport["qemu"] = qmp(vm, "query-version")
                        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as qt:
                            qt.connect(str(tmp / "qtest.sock"))
                            with qt.makefile("rwb", buffering=0) as stream:
                                rapport["pci_initial"] = pci_etat(stream, capture(vm,out,"reset",False))
                                qmp(vm, "cont")
                                echeance = time.monotonic() + (1200 if a.cas == "duo-cycles" else 65)
                                while time.monotonic() < echeance and vm.poll() is None:
                                    serie = (out/"serie.log").read_bytes()
                                    if a.instantane and "nvme_actif" not in rapport and b"E3D WAIT NIC" in serie:
                                        qmp(vm, "stop")
                                        rapport["nvme_actif"] = retrait_duo.temoin_actif(vm, stream, serie)
                                        qmp(vm, "cont")
                                    if any(s in serie for s in (b"NVME bilan", b"NVME admission refusee", b"SMP arret motif=", b"Panique du noyau", b"Exception noyau")) or pair.error:
                                        break
                                    time.sleep(.002 if a.instantane else .1)
                                qmp(vm, "stop")
                                rapport["pci_final"] = pci_etat(stream, capture(vm,out,"fin",False))
                                if panne_qi:
                                    from virtio_net_duo_attente import registres
                                    rapport["qi_incertaine"] = registres(vm, stream, serie, panne_qi, mode)
                                elif a.cas in retrait_duo.CAS and b"NVME bilan ok=true" in serie:
                                    rapport["qi_finale"] = retrait_duo.materiel(vm, stream, serie, a.cas)
                                if a.cas in ("deces", "duo-sans-reset") and b"NVME bilan ok=true" in serie:
                                    rapport["tables_finales"] = tables_finales(vm, stream, serie, mode)
                        qmp(vm,"quit")
                        vm.wait(timeout=5)
                    finally:
                        if vm.poll() is None:
                            vm.kill()
                            vm.wait()
                        pair.thread.join(3)
            rapport["trames_par_sens"] = pair.frames
            oracle = retrait_duo.verifier if a.cas in retrait_duo.CAS else verifier
            if panne_qi:
                oracle = retrait_duo.attente_commune
            rapport["stockage"] = oracle(out, a.cas, a.lecteur, avant, pair)
            if "sonde_garde_sha256" in construction or (construction.get("gestionnaire_4f") and a.cas != "duo-absent"):
                serie = (out / "serie.log").read_text()
                sonde = re.findall(r"E3E garde readmission=refusee candidat=(\d+) generation=(\d+) disque=(\d+) epoque_ipc=(\d+) cid=(\d+) dma=(\d+)", serie)
                if len(sonde) != 1:
                    raise ValueError("Refus de réadmission en vol absent")
                candidat, generation, disque, epoque_ipc, cid, dma = map(int, sonde[0])
                admissions = re.findall(r"ETH3 session .*?pilote=(\d+) .*?generation_materiel=(\d+)", serie)
                suite = serie.split("E3E garde readmission=refusee")[1]
                cqe = re.search(r"E3D CQE op=(\w+) cid=(\d+)", suite)
                baux = re.findall(r"DUO WAIT sid=48 did=1 instance=(\d+) generation=(\d+)", serie)
                if (len(admissions) != 2 or admissions[1] != (str(candidat), str(generation))
                        or admissions[0][0] == str(candidat) or generation != 2 or dma != 14
                        or baux != [(str(disque),"1")]*2 or epoque_ipc <= 0 or not cqe or int(cqe[2]) != cid
                        or "ancienne_capacite_valide=false" not in serie):
                    raise ValueError("CID, titulaires, ressources ou bail NVMe divergents")
                rapport["garde"] = dict(candidat=candidat, generation=generation, disque=disque,
                    generation_vtd=int(baux[0][1]), epoque_ipc=epoque_ipc, cid=cid, commande=cqe[1], dma=dma, cqe_retrouvee=True)
            rapport["p9_final"] = digest(out/"namespace.raw")
            if a.instantane and "nvme_actif" not in rapport:
                raise ValueError("Instantané des domaines en cours de retrait absent")
            rapport["succes"] = True
    finally:
        dump(out/"campagne.json", rapport)


if __name__ == "__main__":
    main()
