"""Observateur de la transition du seul firmware de test épinglé. Aucun périphérique hôte."""

import argparse
from hashlib import sha256
import importlib.util
import json
import os
from pathlib import Path
import re
import resource
import socket
import struct
import subprocess
import sys
import tarfile
import time
import uuid

NOVA = Path(__file__).resolve().parents[2]
K = NOVA / "boot/diagnostic-5k"
sys.path.insert(0, str(K))
spec = importlib.util.spec_from_file_location("banc_5k", K / "banc.py")
k = importlib.util.module_from_spec(spec)
spec.loader.exec_module(k)
from observation_apic import Debug, TRACE, TIMER, EFI, mem
from sonde import hmp, cpu_flatview
from verdict_apic import juger

MODULES = {
    "SmmControl2Dxe": "1ff13b9f8e0bf2613cf5b3a58696b283204222d10877189eed102a61ed078988",
    "VariableSmmRuntimeDxe": "04a0dc535b16bed417cef6a0176d355a196179039cb53f225f2205f71483004b",
    "LocalApicTimerDxe": TIMER,
}
FLAGS = ("SecureBoot", "SetupMode", "AuditMode")


def check(ok, reason):
    if not ok:
        raise ValueError(reason)


def save(name, value):
    Path(name).write_text(json.dumps(value, indent=2) + "\n")


def cpu(dbg):
    return struct.unpack_from("<17Q", bytes.fromhex(dbg.command("g")))


def ram(q):
    tree = cpu_flatview(hmp(q, "info mtree -f"))
    Path("vue.txt").write_text(tree)
    spans = []
    for line in tree.splitlines():
        hit = re.fullmatch(
            r"\s*([0-9a-f]+)-([0-9a-f]+) \(prio -?\d+, ram\): pc\.ram(?: @.*)?\s*", line
        )
        if hit:
            a, b = (int(x, 16) for x in hit.groups())
            check(
                a % 4096 == 0 and (b + 1) % 4096 == 0 and b < 256 << 20, "Étendue RAM"
            )
            spans.append((a, b + 1))
    check(sum(b - a for a, b in spans) > 128 << 20, "RAM ordinaire indéterminée")
    return spans


def instantane(q, spans, signature=None):
    pages = bytearray()
    matches = []
    fichier = Path("lecture-privee.bin")
    for a, b in spans:
        q.call("pmemsave", val=a, size=b - a, filename=str(fichier))
        try:
            donnees = fichier.read_bytes()
            check(len(donnees) == b - a, "Cliché incomplet")
            if signature:
                matches += [
                    a + m.start() - 0x1423
                    for m in re.finditer(re.escape(signature), donnees)
                    if (a + m.start() - 0x1423) % 4096 == 0
                ]
            vue = memoryview(donnees)
            pages.extend(
                b"".join(
                    sha256(vue[i : i + 4096]).digest() for i in range(0, len(vue), 4096)
                )
            )
        finally:
            fichier.unlink(missing_ok=True)
    return pages, matches


def observer(q, hw, mode, refus, cal, pe, modules):
    trace = Path("trace.log")
    d = Debug()
    entry = cal["entree"]
    d.watch(entry)
    d.resume()
    regs = cpu(d)
    check(regs[16] == entry, "Entrée EFI")
    d.remove(entry)
    horloge = time.monotonic_ns()
    b = cal["image"]
    term = b + 0x3578
    check(
        mem(d, entry, 24) == pe[entry - b - 0xC00 : entry - b - 0xC00 + 24]
        and mem(d, term, 3) == b"\xf4\xeb\xfd",
        "PE chargé",
    )
    rt = int.from_bytes(mem(d, regs[3] + 0x58, 8), "little")
    get = int.from_bytes(mem(d, rt + 0x48, 8), "little")
    check(
        mem(d, get, 32) == modules["VariableSmmRuntimeDxe"][0x1494:0x14B4],
        "Service GetVariable",
    )
    check(
        mem(d, get - 0x1494 + 0x113C, 0x2E)
        == modules["VariableSmmRuntimeDxe"][0x113C:0x116A],
        "Site de communication MM",
    )
    spans = ram(q)
    sig = modules["SmmControl2Dxe"][0x1423:0x1468]
    _, matches = instantane(q, spans, sig)
    (control,) = matches
    check(
        mem(d, control, 2) == b"MZ" and mem(d, control + 0x1423, len(sig)) == sig,
        "Module SMM Control",
    )
    (timer,) = cal["timers"]
    for a, z in ((0x4B9, 0x544), (0x962, 0x985)):
        check(
            mem(d, timer + a, z - a) == modules["LocalApicTimerDxe"][a:z],
            "Code minuterie",
        )
    (vcpu,) = q.call("query-cpus-fast")
    check(vcpu["cpu-index"] == 0, "vCPU")
    roles = {
        get: "variable",
        control + 0x1454: "data",
        control + 0x1464: "commande",
        control + 0x1465: "retour",
        timer + 0x4B9: "irq",
        timer + 0x97F: "eoi",
        term: "hlt",
    }
    check(len(roles) == 7, "Points non distincts")
    for pc in roles:
        d.watch(pc)
    evenements = TRACE + ("apm_io_write", "pci_nvme_io_cmd")
    for ev in evenements:
        q.call("trace-event-set-state", name=ev, enable=True)
    debut = trace.stat().st_size
    regs0 = k.state(q)
    compteur0 = q.stats()
    tables0 = hw.tables()
    save("pci-avant.json", regs0)
    Path("tables-avant.bin").write_bytes(b"".join(tables0.values()))
    events = []
    irqs = []
    pending = None
    last = None
    hlt = False
    noms = []
    reprises = []
    adresse_valeur = 0
    for _ in range(192):
        d.resume()
        regs = cpu(d)
        pc = regs[16]
        role = roles[pc]
        off = trace.stat().st_size - debut
        if last is not None and (last != term or (hlt and role == "eoi")):
            d.watch(last)
        d.remove(pc)
        last = pc
        if role == "hlt":
            if hlt:
                break
            hlt = True
            last = None
        elif role == "irq":
            check(pending is None, "Minuterie imbriquée")
            pending = dict(
                entree=off,
                vecteur=regs[2],
                lapic=hmp(q, "info lapic"),
                vcpu=0,
                gestionnaire=pc,
                apres_hlt=hlt,
            )
        elif role == "eoi":
            check(pending is not None, "EOI isolé")
            pending.update(
                avant_ecriture=off,
                instruction=pc,
                adresse=regs[0] + regs[14],
                valeur=regs[1] & 0xFFFFFFFF,
                retour=int.from_bytes(mem(d, regs[7] + 0x58, 8), "little"),
            )
            irqs.append(pending)
            pending = None
            if hlt:
                d.watch(term)
        else:
            if events and role == events[-1]["role"] and pc == events[-1]["pc"]:
                # Une IRQ peut revenir sur l'instruction observée avant son exécution.
                entre = trace.read_bytes()[debut + events[-1]["offset"] : debut + off]
                check(
                    b"Servicing hardware INT=0x20" in entre
                    and b"apm_io_write" not in entre,
                    "Point répété sans retour de minuterie",
                )
                reprises.append(dict(role=role, offset=off))
                continue
            rec = dict(role=role, pc=pc, offset=off)
            if role == "variable":
                check(len(noms) < 3, "Appel GetVariable inattendu")
                nom = FLAGS[len(noms)]
                enc = (nom + "\0").encode("utf-16le")
                check(
                    mem(d, regs[2], len(enc)) == enc
                    and mem(d, regs[3], 16)
                    == uuid.UUID("8be4df61-93ca-11d2-aa0d-00e098032b8c").bytes_le,
                    "Variable hors politique",
                )
                noms.append(nom)
                ret = int.from_bytes(mem(d, regs[7], 8), "little")
                check(
                    b + 0x1000 <= ret < b + 0x31000,
                    "Appel EFI extérieur à la section code",
                )
                if ret not in roles:
                    roles[ret] = "reponse"
                    d.watch(ret)
                check(roles[ret] == "reponse", "Retour ambigu")
                adresse_valeur = int.from_bytes(mem(d, regs[7] + 40, 8), "little")
                check(
                    any(a <= adresse_valeur < z for a, z in spans),
                    "Destination hors RAM",
                )
                rec.update(nom=nom, retour=ret, sortie=adresse_valeur, pile=regs[7])
            if role == "reponse":
                rec.update(
                    statut=regs[0],
                    valeur=mem(d, adresse_valeur, 1)[0] if regs[0] == 0 else None,
                )
                check(regs[0] in (0, 0x800000000000000E), "Statut inattendu")
            if role in ("data", "commande"):
                rec.update(port=regs[3] & 65535, valeur=regs[0] & 255)
                check(
                    rec["port"] == (0xB3 if role == "data" else 0xB2)
                    and rec["valeur"] == 0,
                    "Écriture APM hors profil",
                )
            if role in ("commande", "retour"):
                # Emplacements identifiés dans SendCommunicateBuffer du PE épinglé.
                base_variable = get - 0x1494
                phys, virt = (
                    int.from_bytes(mem(d, base_variable + rva, 8), "little")
                    for rva in (0x41E8, 0x41E0)
                )
                check(
                    phys == virt and any(a <= virt <= z - 40 for a, z in spans),
                    "Canal MM non identifié",
                )
                guid, taille, fonction, statut = struct.unpack(
                    "<16sQQQ", mem(d, virt, 40)
                )
                rec.update(
                    canal=virt,
                    guid=guid.hex(),
                    taille=taille,
                    fonction=fonction,
                    statut_mm=statut,
                )
                tags, _ = instantane(q, spans)
                tag = f"{len(noms)}-{role}"
                Path(tag + ".sha-pages").write_bytes(tags)
                rec["pages_sha256"] = sha256(tags).hexdigest()
            events.append(rec)
    else:
        raise ValueError("Arrêt EFI incomplet")
    check(pending is None and noms == list(FLAGS), "Couverture de la transition")
    fin = trace.stat().st_size
    for ev in evenements:
        q.call("trace-event-set-state", name=ev, enable=False)
    regs1 = k.state(q)
    save("pci-apres.json", regs1)
    tables1 = hw.tables()
    Path("tables-apres.bin").write_bytes(b"".join(tables1.values()))
    check(
        regs0 == regs1
        and tables1 == tables0
        and q.stats() == compteur0
        and not any(compteur0.values()),
        "PCI, DMA ou disque modifiés",
    )
    proof = dict(timer=timer, debut=debut, fin=fin, lignes=irqs, entree=entry, hlt=term)
    raw = trace.read_bytes()
    interval = raw[debut:fin]
    Path("intervalle.log").write_bytes(interval)
    verdict = juger(raw, interval, proof)
    check(
        not verdict["intervalle_admis"] and len(verdict["ecritures_refusees"]) == 6,
        "Politique APM",
    )
    save("apic.json", proof)
    save(
        "observation.json",
        dict(
            evenements=events,
            ram=spans,
            image=b,
            image_fin=b + 0x44D000,
            rss_hote_octets=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
            observation_ns=time.monotonic_ns() - horloge,
            reprises=reprises,
            secure_boot=mode == "nominal",
            paquet_refuse=refus,
            controle=control,
            get=get,
            verdict=verdict,
        ),
    )
    q.observateur = d


def essai(out, j, mode, refus, cal):
    out.mkdir()
    os.chdir(out)
    exe = k.exact(
        j / "modele5j/build/qemu-system-x86_64",
        "085ed32ca433172258c779258f435cb44388e5e6f64d9d8fd816c1fd4f27791e",
    )
    build = j / "ovmf5j-b/Build/OvmfX64/RELEASE_CLANGDWARF"
    fw = k.exact(
        build / "FV/OVMF_CODE.fd",
        "1f767a982e7af4702235e957763ab052f29ae0bdf9f84ee08f03379ef82a70b7",
    )
    nom = "paquet" if refus else "diagnostic"
    fat = k.exact(
        j / "qualification5j" / (nom + ".img"),
        "0ce88d9353f852846833791b94a191555cf5ce21630c23d3bcc0b147d99ce73f"
        if refus
        else "f2783c0d4b93157fd4f46118a11ce6e4f36943f6e8c2ddb62342b1bffefcf88c",
    )
    pe = k.exact(
        j / "supports5j-b" / (nom + ".efi"),
        "fbd459a90d10548b32d96406211f1f87b0d1ad56b8840a033efa4348bd950dbe"
        if refus
        else EFI,
    ).read_bytes()
    modules = {
        n: k.exact(build / "X64" / (n + ".efi"), h).read_bytes()
        for n, h in MODULES.items()
    }
    var = k.exact(
        j / "qualification5j" / (mode + ".fd"),
        {
            "nominal": "e04b60150458454c7f9608209ac7358058c2bb566ddf254e455330c065b1411b",
            "inactif": "8c09b326b45951f539619b4d0f33e5e8fdbafa1cca9b173468f9df7e4a7725e0",
        }[mode],
    )
    epoch = uuid.uuid4().bytes
    Path("epoch.bin").write_bytes(b"NOVA5J-BOOT-EPOC" + epoch)
    disk = Path("interne.raw")
    disk.write_bytes(sha256(b"namespace 5L").digest() * 131072)
    digest = sha256(disk.read_bytes()).hexdigest()
    with (
        open("trace.log", "xb") as log,
        subprocess.Popen(
            k.arguments_q35(exe, fw, fat, var),
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=log,
            bufsize=0,
        ) as vm,
    ):
        try:
            q = k.Qmp(vm)
            (dimm,) = q.call("query-memory-devices")
            check(
                dimm["type"] == "nvdimm"
                and dimm["data"]["id"] == "labo"
                and dimm["data"]["addr"] >= 1 << 32,
                "Réserve",
            )
            (base,) = k.state(q)["unites"]
            pci = list(k.devices(q.call("query-pci")))
            check(all(d["bus"] == 0 for d in pci), "Alias hors profil")
            ids = {
                d["qdev_id"]: d["slot"] * 8 + d["function"] for d in pci if d["qdev_id"]
            }
            with socket.socket(socket.AF_UNIX) as so:
                so.connect("qt")
                with so.makefile("rwb", buffering=0) as io:
                    k.couverture(
                        k.dmar(io),
                        int(base, 16),
                        {d["slot"] * 8 + d["function"] for d in pci},
                    )
                    hw = k.Materiel(q, io, int(base, 16), dimm["data"]["addr"], ids)
                    hw.installer()
                    observer(q, hw, mode, refus, cal, pe, modules)
                    q.call(
                        "pmemsave",
                        val=cal["image"],
                        size=0x44D000,
                        filename="image-privee.bin",
                    )
                    image = Path("image-privee.bin").read_bytes()
                    Path("image-privee.bin").unlink()
                    (pos,) = [
                        m.start()
                        for m in re.finditer(b"NOVA5J-RAM-FINAL", image)
                        if struct.unpack_from("<Q", image, m.start() + 16)[0]
                        == cal["image"] + m.start()
                    ]
                    rec = image[pos : pos + 80]
                    Path("recu.bin").write_bytes(rec)
                    _, _, _, code, sb, _, vivants, echo = struct.unpack(
                        "<16s6Q16s", rec
                    )
                    check(
                        code == (14 if refus else 0)
                        and sb == int(mode == "nominal")
                        and vivants == 0
                        and echo == (bytes(16) if refus else epoch),
                        "Reçu terminal",
                    )
            q.call("quit")
            vm.wait(timeout=5)
        finally:
            if vm.poll() is None:
                vm.kill()
                vm.wait()
    check(sha256(disk.read_bytes()).hexdigest() == digest, "Namespace")
    save("resultat.json", dict(epoque=epoch.hex(), statut=code, namespace=digest))


def main():
    p = argparse.ArgumentParser()
    p.add_argument("build5j", type=Path)
    p.add_argument("sortie", type=Path)
    p.add_argument(
        "--chercher-minuterie",
        type=int,
        default=0,
        metavar="N",
        help="Jusqu'à N démarrages pour observer une minuterie au retour SMM, avec les interruptions naturelles",
    )
    a = p.parse_args()
    if not 0 <= a.chercher_minuterie <= 32:
        p.error("Nombre de démarrages hors bornes")
    j = a.build5j.resolve()
    out = a.sortie.resolve()
    out.mkdir(parents=True, exist_ok=False)
    archive = NOVA / "docs/validation/diagnostic-uefi-5k/observations.tar.gz"
    sums = (archive.parent / "SHA256SUMS").read_text()
    check(
        sha256(archive.read_bytes()).hexdigest() + "  " + archive.name in sums,
        "Calibration 5K",
    )
    with tarfile.open(archive) as t, k.qualification(NOVA.parents[1], {}):
        if a.chercher_minuterie:
            cal = json.load(t.extractfile("inactif/calibration.json"))
            for i in range(1, a.chercher_minuterie + 1):
                dest = out / f"essai-{i}"
                essai(dest, j, "inactif", True, cal)
                obs = json.loads((dest / "observation.json").read_text())
                raw = (dest / "intervalle.log").read_bytes()
                rows = obs["evenements"]
                for n in range(3):
                    fenetre = raw[rows[5 * n + 2]["offset"] : rows[5 * n + 4]["offset"]]
                    if re.search(
                        rb"SMM: after RSM\n(?:(?!memory_region_ops_write|SMM:).)*?apic_local_deliver vector 0 delivery mode 0\nServicing hardware INT=0x20",
                        fenetre,
                        re.S,
                    ):
                        save(
                            out / "selection.json",
                            dict(essais=i, dossier=dest.name, variable=FLAGS[n]),
                        )
                        return
            raise ValueError("Minuterie adjacente non observée dans la série bornée")
        for mode in ("nominal", "inactif"):
            cal = json.load(t.extractfile(mode + "/calibration.json"))
            for refus in (False, True):
                essai(out / (mode + ("-refus" if refus else "")), j, mode, refus, cal)


if __name__ == "__main__":
    main()
