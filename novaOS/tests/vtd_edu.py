"""Qualifie l'EDU et l'unité VT-d sur Q35 via QMP, traces QEMU et série invitée."""
import argparse
import base64
import gzip
import hashlib
from itertools import groupby
import json
import os
from pathlib import Path
import re
import struct
import subprocess
import tempfile
import time

from demarrage import BUILD, ROOT, qmp
from materiel_pilote import read_config, sha256

NORMAL = ["Normal", "WarmWrite", "WrongWrite", "WrongRead", "Unmapped", "Other", "Revoke", "Death", "Late", "Restart"]
QUARANTINE = ["Receipt", "Restart"]
PROFILES = {"normal": (1, 2, 4), "quarantaine": (2,), "erreur-qi": (2,), "sans-iommu": (1,),
            "aw39": (1,), "sans-cm": (1,)}
EVENTS = ("vtd_dmar_fault", "vtd_dmar_translate", "vtd_inv_desc_cc_global",
          "vtd_inv_desc_iotlb_global", "vtd_inv_desc_wait_sw", "vtd_irq_generate")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def physical32(vm, address):
    reply = qmp(vm, "human-monitor-command", **{"command-line": f"xp /1wx {address:#x}"})
    result = re.fullmatch(r"\s*([0-9a-fA-F]+):\s*0x([0-9a-fA-F]+)\s*", reply)
    require(result and int(result[1], 16) == address, f"Relecture physique QMP à {address:#x} : {reply}")
    return int(result[2], 16)


def registers(vm):
    values = {}
    for name, offset, wide in (("ver", 0, False), ("cap", 8, True), ("ecap", 16, True),
                               ("gsts", 0x1c, False), ("root", 0x20, True), ("fsts", 0x34, False),
                               ("fectl", 0x38, False), ("fedata", 0x3c, False),
                               ("feaddr", 0x40, True), ("iqh", 0x80, True),
                               ("iqt", 0x88, True), ("iqa", 0x90, True)):
        at = 0xfed90000 + offset
        values[name] = physical32(vm, at) | (physical32(vm, at + 4) << 32 if wide else 0)
    return values


def table(vm, base, path):
    require(0x1000 <= base <= (256 << 20) - 4096 and base % 4096 == 0,
            f"Page de table VT-d hors RAM du profil : {base:#x}")
    qmp(vm, "pmemsave", val=base, size=4096, filename=str(path))
    data = path.read_bytes()
    require(len(data) == 4096, "Table incomplète via QMP")
    return data


def oracle(vm, profile, cpus, directory):
    devs = [d for bus in qmp(vm, "query-pci") for d in bus["devices"]]
    expected = {(0, 0), (6, 0), (31, 0), (31, 2), (31, 3)}
    require({(d["slot"], d["function"]) for d in devs} == expected and len(devs) == len(expected)
            and all(d["bus"] == 0 and "pci_bridge" not in d for d in devs), "Topologie Q35 inattendue")
    edu = next(d for d in devs if d["slot"] == 6)
    require(edu["id"]["vendor"] == 0x1234 and edu["id"]["device"] == 0x11e8
            and edu["qdev_id"] == "edu-phase3" and len(edu["regions"]) == 1
            and edu["regions"][0]["size"] == 1 << 20, "Identité ou taille EDU inattendue")
    commands = {f"{slot:02x}.{fn}": read_config(vm, "x86_64", 4, slot, fn) & 0xffff
                for slot, fn in sorted(expected)}
    failed_qi = profile == "erreur-qi"
    require(all(not v & 4 for v in commands.values()) and not commands["06.0"] & 7,
            f"Fonction encore armée : {commands}")
    require(read_config(vm, "x86_64", 0) == 0x11e81234, "Identité EDU en ECAM")
    require(read_config(vm, "x86_64", 0x10) == 0x80000000, "BAR0 EDU modifiée")
    indices = sorted(c["cpu-index"] for c in qmp(vm, "query-cpus-fast"))
    require(indices == list(range(cpus)), "Nombre de CPU inattendu")
    result = {"qemu": qmp(vm, "query-version"), "cpu": indices, "pci": commands,
              "edu": edu, "bar0": 0x80000000}
    if profile == "sans-iommu":
        return result
    regs = registers(vm)
    result["unite"] = regs
    require(regs["ver"] == 0x10 and regs["fsts"] == (0x10 if failed_qi else 0), "Version ou statut de faute final VT-d")
    if profile in ("aw39", "sans-cm"):
        require(not any(regs[n] for n in ("root","gsts","iqa","iqh","iqt")),
                "Une unité refusée a été programmée")
        width = (regs["cap"] >> 16 & 63) + 1
        require((width == 39 and profile == "aw39") or
                (width == 48 and profile == "sans-cm" and not regs["cap"] & (1 << 7)),
                "Capacités de l'unité témoin incohérentes")
        return result
    require(regs["gsts"] == 0xc4000000 and regs["iqh"] % 16 == 0
            and 0 < regs["iqa"] < 256 << 20 and regs["iqa"] % 4096 == 0,
            "Traduction legacy ou file d'invalidation non confirmée")
    require(regs["fectl"] == (1 << 31 if failed_qi else 0) and regs["fedata"] == 0x53 and regs["feaddr"] == 0xfee00000,
            "Vecteur de faute incohérent dans l'unité")
    fr = 0xfed90000 + ((regs["cap"] >> 24) & 1023) * 16
    require(not physical32(vm, fr + 12) & (1 << 31), "FRCD encore valide en fin d'essai")
    root = table(vm, regs["root"], directory / "root.bin")
    entry, = struct.unpack_from("<Q", root)
    require(entry & 4095 == 1 and not any(root[8:]), "Racine contenant une autre ouverture")
    context = table(vm, entry & ~4095, directory / "context.bin")
    require(not any(context), "Un contexte de bus zéro subsiste")
    queue = table(vm, regs["iqa"], directory / "file.bin")
    triplets = {"normal": 2+2*len(NORMAL), "quarantaine": 1+2*len(QUARANTINE), "erreur-qi": 3}[profile]
    complete = triplets - int(failed_qi)
    require(regs["iqh"] == complete*48 and regs["iqt"] == triplets*48 and not any(queue[regs["iqt"]:]),
            "Géométrie et consommation des descripteurs")
    receipt = None
    for index in range(triplets):
        context_desc, cc_hi, iotlb_desc, iotlb_hi, wait, address = struct.unpack_from("<6Q", queue, index * 48)
        cc_expected = 0x01 if failed_qi and index == complete else 0x11
        require((context_desc, cc_hi, iotlb_desc, iotlb_hi, wait) == (cc_expected, 0, 0xd2, 0, ((index+1) << 32) | 0x25),
                f"Descripteurs contexte/IOTLB DR-DW/WAIT à l'indice {index}")
        require(0x1000 <= address < (256 << 20) and not (address & 4095) and address not in (
            regs["root"], entry & ~4095, regs["iqa"]), "Destination WAIT hors page de service distincte")
        require(receipt is None or address == receipt, "Adresse de reçu WAIT instable")
        receipt = address
    value = physical32(vm, receipt)
    require(value == (0 if failed_qi else triplets), "Dernier reçu WAIT divergent du registre de tête")
    result["file_invalidation"] = {"sha256": hashlib.sha256(queue).hexdigest(), "triplets_soumis": triplets,
                                   "triplets_consommes": complete,
                                   "adresse_recu": receipt, "dernier_recu": value}
    result["racine"] = {"sha256": hashlib.sha256(root).hexdigest(), "bus0": entry, "autres_octets_nuls": True}
    result["contextes"] = {"sha256": hashlib.sha256(context).hexdigest(), "octets_nuls": 4096}
    return result


def validate(serial, trace, profile, cpus):
    require("VTD bilan ok=true" in serial and "VTD bilan ok=false" not in serial, "Échec du bilan invité")
    require("Panique du noyau" not in serial and "VTD echec " not in serial, "Erreur noyau")
    require("VTD EDU reprise moteur_vierge=true BME=0" in serial and "VTD AHCI ports=6 " in serial,
            "Reprise des maîtres PCI sans attestation du moteur")
    released = re.findall(r"^PROC pid=(\d+) niveau=Ring3 pc=0x[0-9a-f]+ sortie=0\s*$", serial, re.M)
    require(len(set(released)) == len(released), "Identifiant de processus dupliqué")
    if profile == "erreur-qi":
        require(released == ["1"] and "VTD erreur QI confirmee cadres_clients=2 cadres_service=8 admissions=0" in serial
                and f"VTD IQE depart_cpu=0 invalidation_cpu={cpus-1}" in serial
                and "VTD ressources " not in serial and "VTD quarantaine WAIT masque" not in serial,
                "Rétention permanente des pages après IQE")
        trace_events = re.findall(r"^(vtd_inv_desc_cc_global|vtd_inv_desc_iotlb_global|vtd_inv_desc_wait_sw) ", trace, re.M)
        require(trace_events == list(EVENTS[2:5])*2, "Deux lots réussis avant arrêt QI sur le troisième")
        require("vtd_dmar_fault" not in trace and "vtd_dmar_translate dev 00:06.00" in trace, "DMA préalable ou FRCD inattendu")
        require(trace.count("vtd_irq_generate addr 0xfee00000 data 0x53") == 1, "IRQ IQE")
        return {"triplets_invalidation_acquittes": 2, "irq_qemu": 1, "qi_en_erreur": True,
                "cadres_clients_conserves": 2, "pid": list(map(int,released))}
    negative = profile in ("sans-iommu", "aw39", "sans-cm")
    if negative:
        require(len(released) == 1 and "EDU DMA Ring3 sans capacite refuse" in serial
                and "VTD temoin sans domaine ring3_refuse=true pci_ferme=true" in serial
                and "VTD actif " not in serial, "Refus sans attribution non établi")
        marker = {"sans-iommu": "VTD DMAR presente=false", "aw39": "VTD DMAR presente=true unite_admise=false",
                  "sans-cm": "VTD initialisation refusee Feature"}[profile]
        require(marker in serial and ("VTD registre " in serial) == (profile == "sans-cm"),
                "Motif ou étape de refus imprévu")
        require(all(e not in trace for e in EVENTS), "Travail VT-d engagé malgré le refus")
        return {"pid": list(map(int, released)), "refus": True}
    cases = NORMAL if profile == "normal" else QUARANTINE
    rows = re.findall(r"^VTD cas=(\w+) pid=(\d+) epoque=(\d+) ok=true\s*$", serial, re.M)
    require(serial.count("VTD preparation annulee aucun cadre publie") == 1, "Annulation de la première préparation")
    require([r[0] for r in rows] == cases and [int(r[2]) for r in rows] == list(range(2,len(cases)+2)),
            f"Liste des époques : {rows}")
    require(all(r[1] in released for r in rows) and len(released) == len(cases) + (2 if profile == "normal" else 0),
            "Terminaisons Ring 3 incomplètes")
    require("VTD ressources epingles_service=8 epingles_clients=0 ipc=0 tas_restitue=true reemploi=true" in serial,
            "Épinglage, collecte et réemploi")
    require("VTD actif mode=legacy niveaux=4 DID=1 SID=48 service=8" in serial, "Mode de traduction")
    moves = re.findall(r"VTD retrait en_vol=true mort=(true|false) depart_cpu=0 invalidation_cpu=(\d+)", serial)
    require(moves == ([('false',str(cpus-1)), ('true',str(cpus-1))] if profile == "normal"
                      else [('false',str(cpus-1))]), f"Retrait SMP : {moves}")
    if profile == "normal":
        for marker in ("EDU DMA Ring3 page temoin privee 4096 octets intacts",
                       "EDU DMA Ring3 domaine concurrent refuse", "EDU DMA Ring3 ancienne capacite refusee",
                       "EDU DMA Ring3 sortie volontaire apres soumission", "VTD demande tardive refusee avant doorbell"):
            require(marker in serial, marker)
        probes = re.findall(r"VTD observation cas=(\w+) sentinelles=(true|false) faute_enregistree=(true|false)", serial)
        require(probes == [(case, "true", "false" if case == "WarmWrite" else "true")
                           for case in ("WarmWrite","WrongWrite","WrongRead","Unmapped","Other")],
                f"Sentinelles et FSTS indépendants : {probes}")
    else:
        require("VTD quarantaine WAIT masque cadres_epingles=2 nouvelles_admissions=0" in serial, "Quarantaine absente")
    faults = re.findall(r"VTD faute sid=(\d+) did=(None|Some\(1\)) iova=(0x[\da-f]+) "
                        r"lecture=(true|false) raison=(\d+) statut=0x2 irq=(\d+)", serial)
    require(len(faults) == (5 if profile == "normal" else 0), f"Fautes capturées : {faults}")
    if faults:
        require(all(int(r[0]) == 48 for r in faults) and [r[1] for r in faults] == ["None"] + ["Some(1)"]*4 and
                [r[3] for r in faults] == ["false","false","true","false","false"] and
                [int(r[4]) for r in faults] == [2,5,6,5,5] and
                [int(r[5]) for r in faults] == list(range(1, 6)), "Source/séquence IRQ")
    # Le modèle QEMU observe les transactions de l'autre côté des tables invitées.
    actual = re.findall(r"vtd_dmar_fault sid (0x[\da-f]+) fault (-?\d+) addr (0x[\da-f]+) write ([01])", trace)
    observed = [(int(s,16), abs(int(r)), int(a,16), w == "1") for s,r,a,w in actual]
    guest = [(int(s), int(r), int(a,16), read == "false") for s,_,a,read,r,_ in faults]
    groups = []
    calls = []
    for position, callbacks in groupby(observed, lambda row: (row[0],row[1],row[2] & ~4095,row[3])):
        addresses = [row[2] for row in callbacks]
        require(addresses[0] == position[2], "Adresse initiale du groupe de fautes QEMU")
        groups.append(position)
        calls.append(len(addresses))
    require(groups == guest, f"Fautes par page dans QEMU et l'invité : {groups} / {guest}")
    deliveries = re.findall(r"vtd_irq_generate addr (0x[\da-f]+) data (0x[\da-f]+)", trace)
    require([(int(a,16),int(d,16)) for a,d in deliveries] == [(0xfee00000,0x53)] * len(guest),
            f"Interruptions physiques de l'unité : {deliveries}")
    synchronizations = [line for line in trace.splitlines() if any(event in line for event in EVENTS[2:5])]
    order = [next(i for i,e in enumerate(EVENTS[2:5]) if e in line) for line in synchronizations]
    expected_triplets = 1 + 2*len(cases) + (1 if profile == "normal" else 0)
    require(order == [0,1,2] * expected_triplets, f"Ordre de retrait contexte/IOTLB/WAIT : {order}")
    cookies = re.findall(r"vtd_inv_desc_wait_sw wait invalidate status write addr (0x[\da-f]+) data (0x[\da-f]+)", trace)
    require(len(set(a for a,_ in cookies)) == 1 and [int(c,16) for _,c in cookies] == list(range(1,expected_triplets+1)),
            f"Reçus WAIT : {cookies}")
    translations = re.findall(r"vtd_dmar_translate dev 00:06\.00 iova (0x[\da-f]+) -> gpa (0x[\da-f]+)",trace)
    require(translations, "Aucune traduction effective observée par QEMU")
    return {"cas": cases, "pid": list(map(int, released)), "fautes": faults,
            "callbacks_faute_qemu_par_page": calls, "irq_qemu": len(deliveries),
            "triplets_invalidation_acquittes": len(order)//3, "retours_traduction_qemu": len(translations)}


def run(profile, cpus, iso, elf, parent):
    directory = Path(tempfile.mkdtemp(prefix=f"{profile}-{cpus}cpu-", dir=parent))
    print(f"Preuves {directory}", flush=True)
    log = directory / "serie.log"
    env = dict(os.environ, NOVA_ARCH="x86_64", NOVA_VCPUS=str(cpus), NOVA_VGA="none", NOVA_RAM="256",
               NOVA_ISO=str(iso), NOVA_X86_CPU="max")
    options = ["-device", "edu,id=edu-phase3,bus=pcie.0,addr=06.0"]
    if profile != "sans-iommu":
        width = 39 if profile == "aw39" else 48
        caching = "off" if profile == "sans-cm" else "on"
        options += ["-device", f"intel-iommu,id=unite,aw-bits={width},intremap=off,eim=off,"
                    f"caching-mode={caching},device-iotlb=off,scalable-mode=off,dma-translation=on"]
    for event in EVENTS:
        options += ["-trace", f"enable={event}"]
    argv = ["sh", ROOT / "scripts/qemu.sh", "-display", "none", "-monitor", "none",
            "-qmp", "stdio", "-serial", f"file:{log}", *options]
    with (directory / "qemu.log").open("wb") as errors, subprocess.Popen(
            argv,env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=errors,bufsize=0) as vm:
        try:
            qmp(vm, "qmp_capabilities")
            end = time.monotonic() + 100
            while vm.poll() is None and time.monotonic() < end:
                serial = log.read_text(errors="replace") if log.exists() else ""
                if re.search(r"^VTD bilan ok=(true|false)\r?$",serial,re.M) or "Panique du noyau" in serial:
                    break
                time.sleep(.04)
            else:
                raise RuntimeError(f"Bilan absent : {log}")
            qmp(vm, "stop")
            require("VTD bilan ok=true" in serial, f"Échec invité avant restitution dans {log}\n{serial[-1200:]}")
            snapshot = oracle(vm, profile, cpus, directory)
            qmp(vm, "quit")
            require(vm.wait(timeout=5) == 0, "Sortie QEMU")
        finally:
            if vm.poll() is None:
                vm.kill()
                vm.wait()
    trace = (directory / "qemu.log").read_text(errors="replace")
    summary = validate(serial, trace, profile, cpus)
    packed_trace = gzip.compress((directory / "qemu.log").read_bytes(), mtime=0)
    report = {"profil": profile, "vcpus": cpus, "options": options, "resultats": summary,
              "qmp": snapshot, "sha256_elf": sha256(elf), "sha256_iso": sha256(iso),
              "sha256_serie": sha256(log), "serie": log.read_bytes().decode(),
              "sha256_qemu": sha256(directory / "qemu.log"),
              "journal_qemu_gzip_base64":base64.b64encode(packed_trace).decode("ascii")}
    (directory / "bilan.json").write_text(json.dumps(report, ensure_ascii=False,indent=2)+"\n")
    print(f"OK VT-d {profile}, {cpus} vCPU", flush=True)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=PROFILES)
    parser.add_argument("--vcpus", type=int, choices=(1,2,4))
    parser.add_argument("--elf", type=Path, default=os.environ.get("NOVA_KERNEL_ELF"))
    parser.add_argument("--preuves", type=Path, default=BUILD / "vtd-phase3")
    args = parser.parse_args()
    elf = (args.elf or ROOT / "target/x86_64-unknown-none/release/novaos").resolve(strict=True)
    args.preuves.mkdir(parents=True, exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix="campagne-",dir=args.preuves.resolve()))
    env = dict(os.environ, NOVA_ARCH="x86_64", NOVA_KERNEL_ELF=str(elf))
    rows = []
    profiles = (args.profile,) if args.profile else PROFILES
    for profile in profiles:
        boot = {"quarantaine": "vtd-edu-quarantaine", "erreur-qi": "vtd-edu-erreur-qi"}.get(profile, "vtd-edu-test")
        iso = directory / f"{boot}.iso"
        if not iso.exists():
            subprocess.run(["sh", ROOT / "scripts/iso.sh",boot,iso],check=True,env=env,stdout=subprocess.DEVNULL)
        for count in (args.vcpus,) if args.vcpus else PROFILES[profile]:
            rows.append(run(profile,count,iso,elf,directory))
    path = directory / "bilan.json"
    path.write_text(json.dumps(rows, ensure_ascii=False, indent=2)+"\n")
    print(f"Bilan complet {path}")


if __name__ == "__main__":
    main()
