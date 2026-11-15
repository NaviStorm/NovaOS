"""Qualifier le pilote NVMe en lecture seule avec QEMU x86_64 et une image RAW privée."""
import argparse
import base64
import gzip
import json
import os
from pathlib import Path
import re
import shutil
import socket
import subprocess
import tempfile
import time
from types import SimpleNamespace

from demarrage import BUILD, ROOT, qmp
from materiel_pilote import read_config, sha256
from nvme_domaine import observe
from vtd_edu import physical32, require

IMAGE_BYTES = 1 << 20
NEGATIVE = ("absent", "sans-iommu", "identifiant-pci", "bar-supplementaire", "topologie", "heritage")
INCIDENTS = ("mort", "client", "retrait", "delai", "mauvaise-serie", "mauvais-nsid", "permission", "permission-read")
MODES = ("lecture", *INCIDENTS, *NEGATIVE)
EVENTS = ("pci_nvme_admin_cmd", "pci_nvme_io_cmd", "pci_nvme_enqueue_req_completion",
          "pci_nvme_read", "pci_nvme_write", "pci_nvme_flush_ns", "pci_nvme_mmio_cfg",
          "pci_nvme_err_req_status", "vtd_dmar_translate", "vtd_dmar_fault",
          "vtd_inv_desc_cc_global", "vtd_inv_desc_iotlb_global", "vtd_inv_desc_wait_sw")


def dump(path, obj):
    path.write_text(json.dumps(obj, ensure_ascii=False, indent=2) + "\n")


def sample_image(path):
    # Création exclusive dans le répertoire mkdtemp propre à la cellule.
    with path.open("xb") as fd:
        data = bytes((37*i + 13*(i >> 8) + 17*(i >> 16) + 91) & 255 for i in range(IMAGE_BYTES))
        fd.write(data)
    path.chmod(0o400)


def fnv64(data):
    h = 0xcbf29ce484222325
    for b in data:
        h = ((h ^ b) * 0x100000001b3) & ((1 << 64) - 1)
    return f"{h:016x}"


def read_oracle(image, sector):
    sectors = IMAGE_BYTES // sector
    result = []
    with image.open("rb") as fd:
        for lba, blocks in ((0, 1), (sectors - 1, 1), (sectors // 2, 4096 // sector)):
            fd.seek(lba * sector)
            chunk = fd.read(blocks * sector)
            require(len(chunk) == blocks * sector, "Lecture indépendante tronquée")
            result.append((lba, blocks, fnv64(chunk)))
    return result


def hmp(channel, cmd):
    result = qmp(channel, "human-monitor-command", **{"command-line": cmd})
    require(not result.strip(), f"Moniteur QEMU : {cmd} : {result}")


def qemu_command(profile, cpus, sector, iso, image, firmware, serial, control):
    cd = "ide-cd,bus=ide.0,drive=cd"
    boot = "order=d" if profile == "heritage" else "strict=on"
    if profile != "heritage":
        cd += ",bootindex=1"
    cmd = ["qemu-system-x86_64", "-S", "-machine", "pc-q35-11.1", "-accel", "tcg",
           "-cpu", "max", "-smp", str(cpus), "-m", "256M", "-rtc", "base=utc",
           "-nic", "none", "-vga", "none", "-display", "none", "-monitor", "none",
           "-chardev", f"socket,id=control,fd={control}", "-object", "monitor-qmp,id=nvme-qmp,chardev=control",
           "-serial", f"file:{serial}", "-no-reboot",
           "-drive", f"if=pflash,unit=0,format=raw,readonly=on,file={firmware[0]}",
           "-drive", f"if=pflash,unit=1,format=raw,snapshot=on,file={firmware[1]}",
           "-drive", f"if=none,id=cd,format=raw,media=cdrom,readonly=on,file={iso}",
           "-device", cd, "-boot", boot]
    if profile != "sans-iommu":
        cmd += ["-device", "intel-iommu,aw-bits=48,intremap=off,eim=off,caching-mode=on,"
                "device-iotlb=off,scalable-mode=off,dma-translation=on"]
    if profile != "absent":
        sn = "NOVA0190AUTRE" if profile == "mauvaise-serie" else "NOVA0190READ"
        nvme = f"nvme,id=nv,bus=pcie.0,addr=06.0,serial={sn},max_ioqpairs=1,msix_qsize=2"
        if profile == "identifiant-pci":
            nvme += ",use-intel-id=on"
        if profile == "bar-supplementaire":
            nvme += ",cmb_size_mb=1"
        # Le filtre est au-dessous de RAW, au point READ_AIO de ce format.
        backing = f"blkdebug::{image}" if profile == "delai" else str(image)
        ns = 2 if profile == "mauvais-nsid" else 1
        cmd += ["-device", nvme, "-drive", f"if=none,id=source,format=raw,readonly=on,file={backing}",
                "-device", f"nvme-ns,bus=nv,drive=source,nsid={ns},shared=off,"
                f"eui64=0x4e4f564130313930,logical_block_size={sector},physical_block_size={sector}"]
    if profile == "topologie":
        cmd += ["-device", "pcie-root-port,id=racine4,bus=pcie.0,addr=07.0"]
    for event in EVENTS:
        cmd += ["-trace", f"enable={event}"]
    return cmd


def guest_proof(serial, profile, cpus, sector, expected):
    require("Panique du noyau" not in serial, "Panique invitée")
    if profile in NEGATIVE:
        require("NVME admission refusee avant restitution firmware" in serial
                and "NVME acteurs " not in serial and "NVME VT-d SID" not in serial,
                "La reprise devait refuser avant le premier bail")
        return {"admission": "refus avant restitution firmware"}
    malformed = profile in ("mauvaise-serie", "mauvais-nsid")
    require(f"NVME bilan ok={str(not malformed).lower()}" in serial, "Bilan invité incorrect")
    normal = profile == "lecture"
    count = 2 if normal else 1
    actors = re.findall(r"NVME acteurs pilote=(\d+) bloc=(\d+) client=(\d+) cpu_pilote=(\d+) cpu_bsp=(\d+) mode=(\d+)", serial)
    require(len(actors) == count and all(row[3:5] == (str(cpus-1), "0") for row in actors),
            "Nombre de pilotes ou affectation SMP incorrects")
    require(len({p for row in actors for p in row[:3]}) == 3*count, "Identité de processus recyclée")
    require(serial.count("NVME isolation tables_usager=3 cadres_interdits=13 alias=0") == count,
            "Aliasing de pages privées avec les processus du profil")
    if profile != "permission":
        require(serial.count("feuilles=5 droits=R,W,R,W,W autres=0 service=8 BME=0") == count,
                "Domaine non identifié avant activation")
    ids = re.findall(r"NVME Identify Controller vid=0x1b36 vs=0x10400 serial=NOVA0190READ modele=QEMU NVMe Ctrl mdts=(\d+)", serial)
    ns = re.findall(r"NVME Identify Namespace nsid=1 secteurs=(\d+) lba=(\d+) eui64=4e4f564130313930", serial)
    if not malformed and profile != "permission":
        require(len(ids) == count and ns == [(str(IMAGE_BYTES//sector), str(sector))]*count,
                "Identité du contrôleur ou géométrie du namespace incorrecte")
    hashes = re.findall(r"NVME READ fnv64=([0-9a-f]{16})", serial)
    if normal:
        require(hashes == [h for _, _, h in expected]*count, "Empreinte READ différente de la relecture hôte")
        for driver, block, client, *_ in actors:
            for pid in (driver, block, client):
                require(re.search(rf"PROC pid={pid} niveau=Ring3 pc=0x[0-9a-f]+ sortie=0", serial), "Statut Ring 3")
        require(serial.count("client READ premier dernier page octets exacts refus ecriture namespace bornes") == count,
                "Lecture ou refus client manquant")
        require(serial.count("soumises=3 terminees=3 epingles=8 ipc_pages=0") == count
                and serial.count("controleur=confirme BME=0 contexte=0 IOTLB=drain WAIT=confirme pages_restituees=5") == count,
                "Retrait ordonné ou restitution après fermeture absents")
    else:
        require(not hashes, "Des données ont été livrées après l'incident")
        if profile == "delai":
            require("erreur=Some(Timeout)" in serial and "expiration=true" in serial
                    and "client lecture expiree aucun octet livre" in serial, "Échéance manquante")
        if profile == "retrait":
            require("NVME capacite retiree apres doorbell avant collecte" in serial, "Révocation absente")
        if profile == "mort":
            require("NVME pilote sortie apres soumission" in serial, "Mort du pilote absente")
        if profile == "client":
            require("NVME client sortie apres requete de lecture" in serial, "Mort du client absente")
        if profile in ("permission", "permission-read"):
            require("erreur=Some(DmaFault)" in serial, "La faute VT-d doit invalider la requête avant publication")
        if profile == "permission":
            require("NVME isolation Identify destination_absente=true donnees_client=0" in serial and not ids and not ns,
                    "Résultat de l’injection de permission")
        if profile == "permission-read":
            require("NVME isolation READ destination_absente=true donnees_client=0" in serial
                    and "soumises=1 terminees=0" in serial,
                    "Restitution du READ dont le DMA a été bloqué")
        if malformed:
            require("NVME Identify Namespace nsid=" not in serial, "Namespace non autorisé admis")
    memory = []
    for line in re.findall(r"^RAM phase=nvme_[^\r\n]+",serial,re.M):
        fields = dict(re.findall(r"(\w+)=([^ ]+)",line))
        memory.append({"phase": fields["phase"], **{k: int(fields[k]) for k in
                       ("resident","retenu","ipc","dma","processus","utilisateur","kpile","upile","tables")}})
    require(memory, "Mesures RAM absentes")
    return {"acteurs": actors, "mdts": ids, "geometrie": ns, "fnv64_lus": hashes,
            "memoire_pages":memory}


def trace_proof(trace, profile, sector, expected):
    admin = [int(op, 16) for op in re.findall(r"^pci_nvme_admin_cmd cid \d+ sqid 0 opc 0x([0-9a-f]+) ", trace, re.M)]
    io = re.findall(r"^pci_nvme_io_cmd cid \d+ nsid 0x([0-9a-f]+) sqid (\d+) opc 0x([0-9a-f]+) ", trace, re.M)
    read = [(int(lba, 16), int(n), int(size)) for n, size, lba in re.findall(
        r"^pci_nvme_read cid \d+ nsid 1 nlb (\d+) count (\d+) lba 0x([0-9a-f]+)", trace, re.M)]
    if profile == "heritage":
        require(admin and "pci_nvme_mmio_cfg" in trace, "Activité du firmware sur le contrôleur non démontrée")
        return {"commandes_admin_firmware": len(admin), "commandes_io_firmware": len(io)}
    require(all(v == ("1", "1", "2") for v in io) and len(read) == len(io)
            and not re.search(r"^pci_nvme_(write |flush_ns )", trace, re.M),
            "Commande I/O autre que READ")
    sequence = [6, 6, 6, 9, 5, 1]
    fault = None
    if profile in ("permission", "permission-read"):
        events = re.findall(r"^vtd_dmar_fault sid 0x30 fault 5 addr (0x[0-9a-f]+) write 1$", trace, re.M)
        require(events and all(0x02004000 <= int(e,16) < 0x02005000 for e in events),
                "Faute attendue sur la page DATA, SID 48")
        cid, cqid = (1, 0) if profile == "permission" else (7, 1)
        completions = re.findall(rf"^pci_nvme_enqueue_req_completion cid {cid} cqid {cqid} .* status (0x[0-9a-f]+)$", trace, re.M)
        require(len(completions) <= 1, "Complétions en double pour un DMA bloqué")
        fault = {"sid":48, "premiere_iova":int(events[0],16),
                 "tentatives_interdites":len(events),"statuts_completion_qemu":completions}
        if profile == "permission":
            # Ce modèle QEMU peut publier un CQE Identify réussi malgré le refus
            # de toutes les écritures DMA. Le courtier doit rejeter cette requête.
            require(completions == ["0x0"], "Le contrôle doit couvrir le CQE Identify réussi malgré la faute")

    if profile == "lecture":
        require(admin == (sequence + [0, 4])*2, f"Commandes administratives : {admin}")
        require(read == [(lba, count, count*sector) for lba, count, _ in expected]*2,
                f"QEMU a observé d'autres lectures : {read}")
    elif profile == "permission":
        require(admin == [6] and not io and "vtd_dmar_fault" in trace,
                "DMA de destination Identify sans faute observable")
    elif profile == "permission-read":
        require(admin == sequence and read == [(0,1,sector)], "Un seul READ sur la PTE retirée")
    elif profile in NEGATIVE:
        require(not admin and not io, "Commande malgré refus d'admission")
    elif profile in ("mauvaise-serie", "mauvais-nsid"):
        require(admin == ([6] if profile == "mauvaise-serie" else [6, 6]) and not io,
                "Commandes après refus d'Identify")
    else:
        require(admin[:6] == sequence and all(op in (0, 4) for op in admin[6:]), "Initialisation de l'incident")
        require(len(read) == (0 if profile == "client" else 1)
                and all(r == (0, 1, sector) for r in read), f"Lectures avant l'incident : {read}")
    return {"opcodes_admin": admin, "read_lba_nblocs_octets": read, "faute_dma":fault,
            "write": 0, "flush": 0, "traductions_vtd": trace.count("vtd_dmar_translate dev 00:06.0")}


def run(profile, cpus, sector, iso, firmware, root):
    out = Path(tempfile.mkdtemp(prefix=f"{profile}-{cpus}-{sector}-", dir=root))
    print(f"{profile} {cpus} vCPU {sector} octets — {out}", flush=True)
    serial, image = out/"serie.log", out/"image.raw"
    sample_image(image)
    before = sha256(image)
    expected = read_oracle(image, sector)
    require(expected[0][2] != fnv64(bytes(sector)), "LBA témoin indiscernable de zéros")
    early = profile in NEGATIVE
    # Une socket déjà connectée sépare le flux QMP des diagnostics blkdebug de stdout.
    local, child = socket.socketpair()
    local.settimeout(120)
    with local, child, (out/"sortie-qemu.log").open("wb") as stdout, (out/"trace.log").open("wb") as trace:
        command = qemu_command(profile, cpus, sector, iso, image, firmware, serial, child.fileno())
        dump(out/"commande.json", command)
        vm = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=stdout, stderr=trace,
                              pass_fds=(child.fileno(),))
        child.close()
        try:
            with local.makefile("rwb", buffering=0) as stream:
                channel = SimpleNamespace(stdin=stream, stdout=stream)
                qmp(channel, "qmp_capabilities")
                if profile == "delai":
                    hmp(channel, 'qemu-io source "break read_aio attente-nova"')
                qmp(channel, "cont")
                limit = time.monotonic()+110
                while time.monotonic() < limit and vm.poll() is None:
                    log = serial.read_text(errors="replace") if serial.exists() else ""
                    if any(s in log for s in ("NVME bilan ok=", "NVME admission refusee avant restitution firmware", "Panique du noyau")):
                        break
                    time.sleep(.05)
                else:
                    raise RuntimeError(f"Absence de bilan : {out}")
                if profile == "delai":
                    # Le moniteur certifie que la lecture est suspendue, puis la
                    # termine après le bilan invité, pendant que les cadres sont retenus.
                    hmp(channel, 'qemu-io source "wait_break attente-nova"')
                    hmp(channel, 'qemu-io source "resume attente-nova"')
                qmp(channel, "stop")
                pci = [dev for bus in qmp(channel,"query-pci") for dev in bus["devices"]]
                dump(out/"pci.json", pci)
                require(len(qmp(channel,"query-cpus-fast")) == cpus, "Topologie des vCPU")
                if profile != "absent":
                    disks = [b for b in qmp(channel,"query-block") if b.get("device") == "source"]
                    require(len(disks) == 1 and disks[0]["inserted"]["ro"]
                            and disks[0]["inserted"]["drv"] == "raw", "Disque de test non RAW lecture seule")
                guest = guest_proof(log, profile, cpus, sector, expected)
                if early:
                    domain = {}
                    if profile == "heritage":
                        base = (read_config(channel,"x86_64",16) & ~15) | read_config(channel,"x86_64",20) << 32
                        domain = {"bar_physique": base, "cc": physical32(channel,base+0x14),
                                  "csts":physical32(channel,base+0x1c),"aqa":physical32(channel,base+0x24)}
                        require(any(domain[k] for k in ("cc","csts","aqa")), "Ancien contrôleur sans état observable")
                else:
                    held = "NVME quarantaine " in log
                    if profile != "client":
                        require(held == (profile != "lecture"), "Quarantaine inattendue")
                    domain = observe(channel, out, quarantine=held, permission=(profile in ("permission", "permission-read")), serial=log)
                qmp(channel, "quit")
                require(vm.wait(timeout=10) == 0, "Sortie QEMU anormale")
        finally:
            if vm.poll() is None:
                vm.kill()
                vm.wait()
    late = (out/"sortie-qemu.log").read_text(errors="replace")
    if profile == "delai":
        require("Suspended request 'attente-nova'" in late and "Resuming request 'attente-nova'" in late,
                "Point READ_AIO non confirmé par le backend")
    trace_result = trace_proof((out/"trace.log").read_text(errors="replace"),profile,sector,expected)
    after = sha256(image)
    require(before == after, "Empreinte du disque témoin modifiée")
    row = {"profil":profile, "vcpus":cpus, "secteur":sector, "sha256_iso":sha256(iso),
           "sha256_raw_avant":before,"sha256_raw_apres":after,"sha256_serie":sha256(serial),
           "sha256_trace":sha256(out/"trace.log"),"lecture_hote":expected, "observations_invite":guest,
           "qmp":domain,"trace":trace_result,"retard_backend_demontre":profile=="delai"}
    # Les journaux et les pages sources permettent d'auditer la mesure publiée
    # après disparition du répertoire temporaire. L'image se reconstruit par sample_image.
    sources = [serial, out/"trace.log", out/"pci.json", out/"sortie-qemu.log", *sorted(out.glob("nvme-vtd-*.bin"))]
    row["preuves_gzip_base64"] = {p.name: base64.b64encode(gzip.compress(p.read_bytes(), mtime=0)).decode()
                                 for p in sources}
    dump(out/"bilan.json",row)
    return row


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--elf", required=True, type=Path, help="ELF x86_64 du profil nvme-lab")
    parser.add_argument("--profil", action="append", choices=MODES)
    parser.add_argument("--vcpus", choices=(1, 2, 4), type=int)
    parser.add_argument("--secteur", choices=(512, 4096), type=int)
    parser.add_argument("--sortie", type=Path, help="Répertoire de preuves neuf")
    args = parser.parse_args()
    elf = args.elf.resolve(strict=True)
    BUILD.mkdir(exist_ok=True)
    if args.sortie:
        root = args.sortie.resolve()
        root.mkdir(parents=True, exist_ok=False)
    else:
        root = Path(tempfile.mkdtemp(prefix="nvme-phase1-", dir=BUILD))
    executable = shutil.which("qemu-system-x86_64")
    if not executable:
        parser.error("qemu-system-x86_64 introuvable")
    prefix = Path(executable).resolve().parents[1] / "share" / "qemu"
    firmware = [Path(os.environ.get(key, prefix/name)).resolve(strict=True) for key, name in
                (("OVMF_CODE", "edk2-x86_64-code.fd"), ("OVMF_VARS", "edk2-i386-vars.fd"))]
    # Figer l'ELF source une fois pour que toutes les ISO du lot embarquent le même binaire.
    pinned = root / "novaos.elf"
    shutil.copyfile(elf, pinned)
    pinned.chmod(0o400)
    env = dict(os.environ, NOVA_ARCH="x86_64", NOVA_KERNEL_ELF=str(pinned))
    summary = {"sha256_elf": sha256(pinned), "firmware": [{"nom": f.name, "sha256": sha256(f)} for f in firmware],
               "qemu": subprocess.check_output([executable, "--version"], text=True).splitlines()[0], "cellules": []}
    for profile in args.profil or MODES:
        boot = "nvme-lecture" if profile in NEGATIVE or profile in ("mauvaise-serie", "mauvais-nsid") else f"nvme-{profile}"
        iso = root / f"{boot}.iso"
        if not iso.exists():
            with (root / f"{boot}-construction.log").open("wb") as output:
                subprocess.run(["sh", ROOT/"scripts/iso.sh", boot, iso], env=env, stdout=output,
                               stderr=subprocess.STDOUT, check=True)
        cpu_cases = (1, 2, 4) if profile == "lecture" else (2,)
        sector_cases = (512, 4096) if profile == "lecture" else (512,)
        for cpus in (args.vcpus,) if args.vcpus else cpu_cases:
            for sector in (args.secteur,) if args.secteur else sector_cases:
                summary["cellules"].append(run(profile, cpus, sector, iso, firmware, root))
                dump(root / "bilan.json", summary)
    print(root / "bilan.json", flush=True)


if __name__ == "__main__":
    main()
