"""Q35/VT-d : WRITE, FLUSH et reprises de NovaFS P9 sur des RAW privés, sans partitionnement."""
import argparse
import json
import tempfile
from concurrent.futures import ThreadPoolExecutor
import os
from pathlib import Path
import re
import shutil
import socket
import struct
import subprocess
from threading import Lock
import time
from types import SimpleNamespace

from demarrage import ROOT, qmp
from materiel_pilote import read_config, sha256
from nvme_domaine import observe
from nvme_phase2_oracle import (BLOCK, NEW, RAW_BYTES, chronology, inspect, io_trace,
                                unconfirmed_write, verify_transition, view, virgin)
from nvme_qemu import dump, hmp, qemu_command
from vtd_edu import physical32, registers, require, table

CUTS = {
    "before-write": None, "during-write": "write_aio", "after-write": None,
    "during-f1": "flush_to_disk", "after-f1": None, "during-publish": "write_aio",
    "during-f2": "flush_to_disk", "after-f2": None,
}
INCIDENTS = ("rights", "revoke-write", "revoke-flush", "timeout", "driver-death",
             "client-death", "dma-fault", "qi-error", "qi-delay", "race")
EIO = ("eio-write", "eio-f1", "eio-f2")
MODES = ("cycle", *CUTS, *INCIDENTS, *EIO)
ACK = "NVME P9 sauvegarde confirmee"


def select_cases(mode, cpus, sector):
    if mode in CUTS:
        counts, sizes = (2,), (512, 4096)
    elif mode == "cycle":
        counts, sizes = (1, 2, 4), (512, 4096)
    elif mode in ("revoke-write", "revoke-flush", "driver-death", "client-death", "race"):
        counts, sizes = (1, 2, 4), (512,)
    else:
        counts, sizes = (2,), (512,)
    return [(c, s) for c in ((cpus,) if cpus else counts) for s in ((sector,) if sector else sizes)]


def counters(channel):
    drive, = [d for d in qmp(channel, "query-blockstats") if d.get("device") == "source"]
    return {k: drive["stats"][k] for k in ("rd_operations", "wr_operations", "wr_bytes", "flush_operations",
                                                   "rd_total_time_ns", "wr_total_time_ns", "flush_total_time_ns")}


def trace_command(mode, cpus, sector, iso, image, firmware, serial, fd, fault_config=None):
    # Le profil sans endpoint fournit la plateforme Q35 de phase 1. Uniquement
    # l'image RAW nouvellement créée par Bench est présentée comme namespace.
    base = qemu_command("absent", cpus, sector, iso, image, firmware, serial, fd)
    # Un log par traduction ne contribue pas à l'oracle ; conserver les fautes et les WAIT.
    event = base.index("enable=vtd_dmar_translate")
    del base[event-1:event+1]
    readonly = "on" if mode == "recover" else "off"
    rules = f"file.config={fault_config}," if fault_config else ""
    return base + [
        "-device", "nvme,id=nv,bus=pcie.0,addr=06.0,serial=NOVA0190RW,max_ioqpairs=1,msix_qsize=2",
        "-drive", f"if=none,id=source,format=raw,readonly={readonly},cache=writeback,"
        f"file.driver=blkdebug,{rules}file.image.driver=file,file.image.filename={image},rerror=report,werror=report",
        "-device", f"nvme-ns,id=ns,bus=nv,drive=source,nsid=1,shared=off,eui64=0x4e4f564130313930,"
        f"logical_block_size={sector},physical_block_size={sector}"]


def wait_for(vm, serial, marker, deadline):
    while time.monotonic() < deadline and vm.poll() is None:
        text = serial.read_text(errors="replace") if serial.exists() else ""
        if marker in text:
            return text
        if "Panique du noyau" in text or "NVME admission refusee" in text or "NVME bilan ok=" in text:
            raise RuntimeError(f"{marker!r} absent de {serial}: {text[-1000:]}")
        time.sleep(.015)
    raise RuntimeError(f"Échéance en attendant {marker!r}, série conservée : {serial}")


def wait_trace(vm, trace, start, prefix, deadline):
    """Attendre le dispatch matériel sans bloquer le thread QEMU/TCG dans HMP."""
    with trace.open("rb") as fd:
        fd.seek(start)
        pending = b""
        while time.monotonic() < deadline and vm.poll() is None:
            *lines, pending = (pending + fd.read()).split(b"\n")
            if any(row.startswith(prefix) for row in lines):
                return
            time.sleep(.01)
    raise RuntimeError(f"Échéance sans événement {prefix!r} dans {trace}")


def host_caps(channel, cpus, sector, mode):
    require(len(qmp(channel, "query-cpus-fast")) == cpus, "vCPU QMP")
    item, = [d["inserted"] for d in qmp(channel, "query-block") if d.get("device") == "source"]
    require(item["drv"] == "raw" and item["ro"] == (mode == "recover"), "Accès RAW QMP")
    require(item["cache"] == {"writeback": True, "direct": False, "no-flush": False},
            f"Cache incompatible avec le contrat FLUSH: {item['cache']}")
    fields = [qmp(channel, "qom-get", path="/machine/peripheral/ns", property=p)
              for p in ("logical_block_size", "physical_block_size")]
    require(fields == [sector, sector], "Géométrie QMP du namespace")
    return {"ram_mio": 256, "cache": item["cache"], "lecture_seule": item["ro"], "secteurs": fields}


def held_domain(channel, out, mode, serial):
    """L'état de la file QI d'un retrait volontairement interrompu est conservé tel quel."""
    r = registers(channel)
    command = read_config(channel, "x86_64", 4, 6, 0) & 0xffff
    require(command & 0x406 == 0x402, "Quarantaine sans fermeture PCI")
    if mode in ("qi-delay", "qi-error"):
        require(r["gsts"] & (1<<31), "Traduction VT-d désactivée")
        require("quarantaine" in serial and "WAIT=confirme pages_restituees=6" not in serial,
                "Retrait présenté comme certain")
        root = table(channel, r["root"], out/"nvme-vtd-racine.bin")
        ptr, = struct.unpack_from("<Q", root)
        require(ptr & 4095 == 1 and not any(root[8:]), "Une autre racine DMA est ouverte")
        require(not any(table(channel,ptr & ~4095,out/"nvme-vtd-contexte.bin")), "Contexte non retiré")
        ring = table(channel,r["iqa"],out/"nvme-vtd-qi.bin")
        submitted = r["iqt"]//48
        require(2 <= submitted <= 85 and submitted*48 == r["iqt"] and not any(ring[r["iqt"]:]),
                "Géométrie de la file QI de retrait")
        last = submitted-1
        failed = mode == "qi-error"
        for pos in range(submitted):
            cc,zero,tlb,high,wait,address = struct.unpack_from("<6Q",ring,48*pos)
            expect = 0x01 if failed and pos==last else 0x11
            require((cc,zero,tlb,high,wait)==(expect,0,0xd2,0,(pos+1)<<32|0x25),
                    "Descripteurs de contexte/IOTLB/WAIT inattendus")
        value = physical32(channel,address)
        require((not failed and r["fsts"] == 0 and r["iqh"] <= r["iqt"]) or
                (failed and r["iqh"] == last*48 and r["fsts"] == 16 and value == 0),
                "Absence de témoin de l'erreur ou incohérence du reçu")
        return {"registre_pci": command, "fsts": r["fsts"], "iqh": r["iqh"], "iqt": r["iqt"],
                "racine": r["root"], "iq": r["iqa"], "cookie_lu_hote": value,
                "retrait_confirme": False}
    result = observe(channel, out, quarantine=True, serial=serial,
                     writable=True, missing_source=(mode == "dma-fault"))
    if mode == "dma-fault":
        fr = 0xfed90000 + ((r["cap"] >> 24) & 1023)*16
        hi = physical32(channel,fr+8) | physical32(channel,fr+12) << 32
        lo = physical32(channel,fr) | physical32(channel,fr+4) << 32
        require(hi & (1<<63) and hi & (1<<62) and (hi & 0xffff) == 48
                and (hi >> 32 & 255) == 6 and lo == 0x02005000,
                f"Faute DMA inattendue {lo:#x}/{hi:#x}")
        m = re.search(r"NVME refus DMA WRITE SID=48 faute=6 lecture=true iova=(0x[0-9a-f]+) cadre_temoin=(0x[0-9a-f]+) pte=0 source_intacte=true",serial)
        require(m and int(m[1],16) == lo, "Absence de comparaison invitée du cadre source")
        source = table(channel,int(m[2],16),out/"nvme-vtd-source.bin")
        result["source_témoin_sha256"] = sha256(out/"nvme-vtd-source.bin")
        require(source == NEW[:BLOCK], "Le cadre source gelé ne correspond pas aux 4096 premiers octets du fichier")
        result["faute_source"] = {"iova": lo, "frcd_hi": hi}
    return result


def processes(doc,log,mode,cpus,sector):
    participants = re.search(r"NVME acteurs pilote=(\d+) bloc=(\d+) client=(\d+) cpu_pilote=(\d+) cpu_bsp=(\d+)",log)
    require(participants is not None,
            "Acteurs de la chaîne absents")
    driver,block,service,driver_cpu,broker_cpu = map(int, participants.groups())
    require(driver_cpu == cpus-1 and broker_cpu == 0, "Affectation SMP initiale incorrecte")
    ends = {int(pid):int(status) for pid,status in re.findall(r"PROC pid=(\d+) niveau=Ring3 pc=0x[0-9a-f]+ sortie=(\d+)",log)}
    app = re.search(r"DEBUG pid=(\d+) NVME P9 (?:debut transaction|creation confirmee|oracle )",log)
    app_pid = int(app[1]) if app else None
    doc["processus"]={"pilote":driver,"bloc":block,"service_p9":service,"application":app_pid,
                      "retours":{str(pid):ends.get(pid) for pid in (driver,block,service,app_pid) if pid is not None}}
    doc["migration"] = re.findall(r"NVME RW migration pilote cpu_origine=(\d+) cpu_final=(\d+)",log)
    doc["revocation"] = re.findall(r"NVME RW revocation slot=(\d+) cpu=(\d+) en_vol=true",log)
    doc["identite_namespace"] = re.findall(r"NVME Identify Namespace nsid=(\d+) secteurs=(\d+) lba=(\d+) eui64=([0-9a-f]+)",log)
    epochs = 2 if mode=="reuse" else 1
    require(doc["identite_namespace"] == [("1",str(RAW_BYTES//sector),str(sector),"4e4f564130313930")]*epochs,
            "L'identité et la capacité physiques doivent couvrir le RAW jetable")
    if mode in ("init","update","recover","rights","race"):
        require(all(pid in ends and ends[pid]==0 for pid in (driver,block,service,app_pid)),
                "La fin d'une des quatre images Ring 3 n’a pas été confirmée")


def application_order(tx, log):
    rows = list(re.finditer(r"NVME RW FLUSH CQE cid=(\d+) ecritures=\d+ numero=\d+",log))
    by_cid = {int(r[1]):r.start() for r in rows}
    cids = tx["flush_cid"]
    pos = log.index(ACK)
    require(len(cids)==2 and all(cid in by_cid for cid in cids)
            and log.index("NVME P9 debut transaction") < by_cid[cids[0]] < by_cid[cids[1]] < pos,
            "L'acquittement applicatif doit suivre F1, le dépôt de l'ancre et F2")
    return {"f1_cid":cids[0],"f2_cid":cids[1],"confirmation_apres_f2":True}


class Bench:
    def __init__(self, args):
        self.dir, self.reader, self.timeout = args.sortie, args.lecteur, args.delai
        resume = getattr(args, "reprendre", False)
        if not resume:
            self.dir.mkdir(parents=True, exist_ok=False)
        self.elf = self.dir / "novaos.elf"
        if resume:
            require(self.dir.is_dir() and (self.dir/"bilan.json").is_file() and sha256(self.elf)==sha256(args.elf),
                    "La reprise exige l’ELF figé et un bilan initial")
        else:
            shutil.copyfile(args.elf, self.elf)
            self.elf.chmod(0o400)
        executable = shutil.which("qemu-system-x86_64")
        require(executable, "QEMU x86_64 introuvable")
        share = Path(executable).resolve().parents[1] / "share/qemu"
        self.firmware = [Path(os.environ.get(k, share/n)).resolve(strict=True)
                         for k,n in (("OVMF_CODE","edk2-x86_64-code.fd"),("OVMF_VARS","edk2-i386-vars.fd"))]
        self.env = dict(os.environ, NOVA_ARCH="x86_64", NOVA_KERNEL_ELF=str(self.elf))
        self.seeds = {}
        self.lock = Lock()
        self.report = {"sha256_elf": sha256(self.elf), "sha256_lecteur": sha256(self.reader),
                       "firmware": {f.name: sha256(f) for f in self.firmware},
                       "qemu": subprocess.check_output([executable, "--version"], text=True).splitlines()[0],
                       "geometrie": {"raw":RAW_BYTES, "zone_p9":RAW_BYTES-BLOCK}, "initialisations":[], "cellules":[]}

        if resume:
            old = json.loads((self.dir/"bilan.json").read_bytes())
            for key in self.report.keys()-{"initialisations","cellules"}:
                require(self.report[key] == old[key], f"Configuration de reprise différente: {key}")
            self.report = old
            for row in old["initialisations"]:
                sector = row["vm"]["secteur"]
                raw = self.dir/f"initial-{sector}"/"initial.raw"
                require(sha256(raw)==row["vm"]["sha256_raw_apres"], "Fichier d'initialisation altéré")
                self.seeds[sector] = raw, inspect(self.reader,raw,sector)

    def iso(self, mode):
        target = self.dir / f"nvme-rw-{mode}.iso"
        if not target.exists():
            with target.with_suffix(".log").open("xb") as log:
                subprocess.run(["sh", ROOT/"scripts/iso.sh", target.stem, target], env=self.env,
                               stdout=log, stderr=subprocess.STDOUT, check=True)
        return target

    def vm(self, mode, cpus, sector, image, out, frame_probe=None):
        out.mkdir(exist_ok=False)
        iso = self.iso("update" if mode in EIO else mode)
        cfg = None
        if mode in EIO:
            cfg = out/"eio.conf"
            order = {"eio-write": [], "eio-f1":["write_aio"], "eio-f2":["write_aio","flush_to_disk"]}[mode]
            event = "write_aio" if mode == "eio-write" else "flush_to_disk"
            rules = "".join(f'[set-state]\nevent = "{e}"\nstate = "{i}"\nnew_state = "{i+1}"\n'
                            for i,e in enumerate(order, 1))
            cfg.write_text(rules+f'[inject-error]\nevent = "{event}"\nstate = "{len(order)+1}"\nerrno = "5"\nonce = "on"\n')
        serial, trace = out/"serie.log", out/"trace.log"
        start = time.monotonic()
        cut = mode.removeprefix("cut-") if mode.startswith("cut-") else None
        stop = CUTS.get(cut)
        initial = sha256(image)
        late_flush = mode in ("revoke-flush", "timeout-flush")
        late = late_flush or mode in ("timeout","revoke-write","driver-death","client-death")
        samples = {}
        local, child = socket.socketpair()
        local.settimeout(8)
        with local, child, trace.open("xb") as err, (out/"backend.log").open("xb") as stdout:
            cmd = trace_command(mode, cpus, sector, iso, image, self.firmware, serial, child.fileno(),cfg)
            dump(out/"commande.json", cmd)
            vm = subprocess.Popen(cmd, stdin=subprocess.DEVNULL, stdout=stdout, stderr=err, pass_fds=(child.fileno(),))
            child.close()
            try:
                with local.makefile("rwb", buffering=0) as stream:
                    channel = SimpleNamespace(stdin=stream, stdout=stream)
                    qmp(channel,"qmp_capabilities")
                    graph = host_caps(channel,cpus,sector,mode)
                    if mode == "timeout-flush":
                        hmp(channel, 'qemu-io source "break write_aio premier"')
                    elif late:
                        evt = "flush_to_disk" if late_flush else "write_aio"
                        hmp(channel, f'qemu-io source "break {evt} retard"')
                    qmp(channel,"cont")
                    deadline = start+self.timeout
                    if mode == "timeout-flush":
                        wait_trace(vm, trace, 0, b"pci_nvme_write ", deadline)
                        hmp(channel, 'qemu-io source "wait_break premier"')
                        hmp(channel, 'qemu-io source "break flush_to_disk retard"')
                        hmp(channel, 'qemu-io source "resume premier"')
                    if mode == "reuse":
                        require(frame_probe, "La réadmission exige la contrelecture physique des baux")
                        for label,witness in (("entre_incarnations","NVME P3 entre incarnations temoins=6"),
                                              ("avant_retrait_lecture","NVME P3 lectures achevees avant retrait"),
                                              ("avant_liberation","NVME P3 temoins intacts octets=24576")):
                            wait_for(vm,serial,witness,deadline)
                            qmp(channel,"stop")
                            samples[label]=frame_probe(channel,out,serial.read_text(errors="replace"),label)
                            qmp(channel,"cont")
                    marker = f"NVME RW coupe {cut}" if cut else "NVME bilan ok="
                    log = wait_for(vm, serial, marker, deadline)
                    if cut:
                        if stop:
                            pos = trace.stat().st_size
                            hmp(channel, f'qemu-io source "break {stop} coupe"')
                            # HMP wait_break peut retenir le thread principal QEMU/TCG.
                            # L'invité doit d'abord quitter sa porte et atteindre le NVMe.
                            prefix = b"pci_nvme_flush_ns " if stop=="flush_to_disk" else b"pci_nvme_write "
                            wait_trace(vm,trace,pos,prefix,deadline)
                            hmp(channel, 'qemu-io source "wait_break coupe"')
                        disk = counters(channel)
                        # SIGKILL préserve l'état exact du fichier et empêche le FLUSH
                        # implicite que provoquerait une sortie QMP ordonnée.
                        vm.kill()
                        require(vm.wait(timeout=10) == -9, "Arrêt forcé QEMU non confirmé")
                        domain = None
                    else:
                        require(("NVME bilan ok=false" if mode in EIO else "NVME bilan ok=true") in log,
                                f"Bilan invité inattendu : {log[-1100:]}")
                        if late:
                            hmp(channel, 'qemu-io source "wait_break retard"')
                            if frame_probe:
                                samples["avant_cqe_tardive"]=frame_probe(channel,out,log,"avant")
                            hmp(channel, 'qemu-io source "resume retard"')
                        qmp(channel, "stop")
                        disk = counters(channel)
                        held = "NVME quarantaine " in log
                        domain = (held_domain(channel,out,mode,log) if held else
                                  observe(channel,out,quarantine=False,serial=log))
                        if frame_probe:
                            samples["au_bilan"]=frame_probe(channel,out,log,"final")
                        qmp(channel,"quit")
                        require(vm.wait(timeout=10) == 0, "Sortie QEMU")
            finally:
                if vm.poll() is None:
                    vm.kill()
                    vm.wait()
        ops = io_trace(trace, sector, 2 if mode=="reuse" else 1)
        doc = {"mode": mode, "vcpus": cpus, "secteur": sector, "arret": "SIGKILL" if cut else "QMP quit",
               "commandes": len(ops), "write":sum(o["op"]=="WRITE" for o in ops),
               "flush":sum(o["op"]=="FLUSH" for o in ops), "compteurs_qemu": disk,
               "sha256_raw_avant":initial, "sha256_raw_apres":sha256(image), "sha256_iso":sha256(iso),
               "sha256_serie":sha256(serial), "sha256_trace":sha256(trace), "cache_qmp":graph,
               "secondes_ecoulees_hote": round(time.monotonic()-start,3), "domaine":domain,
               "confirmation_client":ACK in log, "breakpoint_confirme":stop or None}
        processes(doc,log,mode,cpus,sector)
        if samples:
            doc["cadres_physiques"]=samples
        if late:
            last = ops[-1] if ops else {}
            notices = (out/"backend.log").read_text()
            require("Suspended request 'retard'" in notices and "Resuming request 'retard'" in notices
                    and last.get("op") == ("FLUSH" if late_flush else "WRITE")
                    and last.get("cqe") == 0 and not doc["confirmation_client"],
                    "La CQE du backend doit devenir observable après la clôture du bail")
            doc["cqe_tardive_apres_bilan"] = last
        view(image)
        dump(out/"bilan.json", doc)
        return doc, ops, log

    def seed(self, sector):
        if sector not in self.seeds:
            root = self.dir / f"initial-{sector}"
            root.mkdir()
            image = root/"initial.raw"
            virgin(image)
            boot, _, log = self.vm("init", 2, sector, image, root/"creation")
            old = inspect(self.reader, image, sector)
            require(old["etat"] == "ancien" and boot["write"] and boot["flush"] >= 2
                    and boot["domaine"]["cadres_clients_mappes"] == [], "Création du volume et retrait NVMe")
            image.chmod(0o400)
            self.seeds[sector] = (image, old)
            self.report["initialisations"].append({"image": old, "vm": boot})
            self.save()
        return self.seeds[sector]

    def case(self, name, cpus, sector):
        template, old = self.seed(sector)
        root = self.dir / f"{name}-{cpus}-{sector}"
        if root.exists():
            root = Path(tempfile.mkdtemp(prefix=root.name+"-rejeu-",dir=self.dir))
        else:
            root.mkdir()
        image = root/"disque.raw"
        shutil.copyfile(template,image)
        mode = "cut-"+name if name in CUTS else "update" if name=="cycle" else name
        boot, ops, log = self.vm(mode,cpus,sector,image,root/"operation")
        after = inspect(self.reader,image,sector)
        row = {"point":name, "vcpus":cpus, "secteur":sector, "dossier":root.name, "operation":boot, "p9":after}
        if name == "cycle" or name in CUTS:
            row["transaction"] = chronology(ops,old,sector,cut=name if name in CUTS else None)
            if name == "cycle":
                row["ordre_acquittement"] = application_order(row["transaction"],log)
            require(boot["confirmation_client"] == (name=="cycle"), "Acquittement client déplacé")
            allowed = ("nouveau",) if name in ("cycle","after-f2") else (
                ("ancien","nouveau") if name in ("during-publish","during-f2") else ("ancien",))
        elif name in EIO:
            row["eio"] = eio_proof(name,boot,ops,log)
            allowed = ("ancien","nouveau") if name=="eio-f2" else ("ancien",)
        else:
            allowed = incident(name, boot, ops, log)
            if name == "dma-fault":
                row["impact_write_sans_confirmation"] = unconfirmed_write(template,image,ops,sector)
        verify_transition(old, after, allowed)
        restore, _, checked = self.vm("recover",cpus,sector,image,root/"remontage")
        require("NVME P9 oracle "+after["etat"] in checked and not restore["write"] and not restore["flush"]
                and restore["sha256_raw_avant"] == restore["sha256_raw_apres"] == boot["sha256_raw_apres"],
                "Remontage froid ou intégrité du témoin")
        row["remontage"] = restore
        dump(root/"bilan.json",row)
        with self.lock:
            self.report["cellules"].append(row)
            self.report["cellules"].sort(key=lambda r:(MODES.index(r["point"]),r["vcpus"],r["secteur"]))
            self.save()

    def save(self):
        path = self.dir/"bilan.temp.json"
        dump(path,self.report)
        path.replace(self.dir/"bilan.json")


def eio_proof(name, boot, ops, log):
    fail = re.search(r"NVME RW erreur commande=(Write|Flush).*? cid=(\d+) statut_nvme=Some\((\d+)\)",log)
    failed = [o for o in ops if o.get("cqe",0)]
    require(fail and len(failed)==1 and (int(fail[2]),int(fail[3]))==(failed[0]["cid"],failed[0]["cqe"])
            and failed[0]["op"] == ("WRITE" if name=="eio-write" else "FLUSH"),
            "Le statut matériel ou le CID en échec divergent du courtier")
    require(not boot["confirmation_client"] and "NVME quarantaine" in log,
            "Retour de succès ou absence de quarantaine après erreur")
    cid, raw = int(fail[2]), int(fail[3])
    pids = boot["processus"]
    outcomes = pids["retours"]
    status = 22 if name=="eio-f2" else 26
    require(outcomes.get(str(pids["application"])) == status and all(
            outcomes.get(str(pids[k]))==26 for k in ("pilote","bloc")),
            "Statut d'échec incorrect au terme du processus")
    delivered = {}
    for label,key,code in (("bloc","bloc",26),("p9","service_p9",status),("app","application",status)):
        marker = f"DEBUG pid={pids[key]} NVME CQE {label} code={code:02x} cid={cid:04x} statut={raw:04x}"
        require(log.count(marker)==1, f"Réponse native absente ou dupliquée au niveau {label}")
        delivered[label] = code
    return {"retours":outcomes, "cid":cid, "statut_nvme":raw, "commande":failed[0]["op"],
            "codes_dans_les_trames":delivered}


def incident(name, boot, ops, log):
    if name == "rights":
        require(not boot["write"] and not boot["flush"] and boot["sha256_raw_avant"]==boot["sha256_raw_apres"],
                "Commande de mutation malgré le droit de lecture seul")
    if name in ("revoke-write","revoke-flush","timeout","driver-death","dma-fault"):
        require(not boot["confirmation_client"] and "quarantaine" in log,
                "Commande incertaine reconnue durable ou cadres réemployés")
        op = "FLUSH" if name == "revoke-flush" else "WRITE"
        require(any(o["op"]==op for o in ops), f"{op} non soumis")
    require("NVME RW droits generation plage taille et adresse verifies" in log,
            "Contrôles capability/namespace/limite/génération absents")
    if name in ("revoke-write","revoke-flush"):
        marker = re.search(r"NVME RW revocation slot=(\d+) cpu=(\d+) en_vol=true",log)
        require(marker and (boot["vcpus"] == 1 or int(marker[2]) != 0), "Révocation inter-CPU absente")
    if name == "driver-death":
        require("NVME RW pilote mort apres WRITE" in log, "Destruction du pilote non observée")
    if name == "client-death":
        require("NVME RW client mort pendant WRITE" in log and not boot["confirmation_client"]
                and "NVME quarantaine " in log, "Mort du client ou retenue des cadres non démontrée")
    if name == "timeout":
        require("expiration=true" in log, "Timeout non observé")
    if name == "race":
        require("NVME P9 ecrivain concurrent refuse ancienne vue verifiee" in log,
                "Rejet du second écrivain non observé")
    if name == "race" and boot["vcpus"] > 1:
        require(f"NVME RW migration pilote cpu_origine={boot['vcpus']-1} cpu_final=0" in log, "Migration non observée")
    if name in ("qi-error","qi-delay"):
        require("quarantaine" in log or "WAIT=confirme pages_restituees=6" in log, "Sort du bail inconnu")
    return ("nouveau",) if boot["confirmation_client"] else ("ancien",)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--elf", required=True, type=lambda s:Path(s).resolve(strict=True))
    p.add_argument("--lecteur", required=True, type=lambda s:Path(s).resolve(strict=True))
    p.add_argument("--sortie", required=True, type=lambda s:Path(s).resolve(),help="Répertoire de preuves neuf")
    p.add_argument("--reprendre",action="store_true",help="Reprendre un lot de même ELF sans modifier les essais précédents")
    p.add_argument("--point", action="append", choices=MODES)
    p.add_argument("--vcpus", type=int, choices=(1,2,4))
    p.add_argument("--secteur", type=int, choices=(512,4096))
    p.add_argument("--jobs",type=int,choices=(1,2,3,4),default=1,help="Nombre de VM de qualification indépendantes")
    p.add_argument("--delai", type=int,default=150,help="Échéance de surveillance d'un démarrage en secondes")
    args = p.parse_args()
    b = Bench(args)
    cases = [(name,c,s) for name in dict.fromkeys(args.point or MODES)
             for c,s in select_cases(name,args.vcpus,args.secteur)]
    complete = {(c["point"],c["vcpus"],c["secteur"]) for c in b.report["cellules"]}
    cases = [cell for cell in cases if cell not in complete]
    for sector in sorted({cell[2] for cell in cases}):
        b.seed(sector)
    for name in dict.fromkeys([cell[0] for cell in cases]+["recover"]):
        b.iso("cut-"+name if name in CUTS else "update" if name=="cycle" or name in EIO else name)

    def run(cell):
        print(*cell,flush=True)
        b.case(*cell)

    errors = []
    with ThreadPoolExecutor(max_workers=args.jobs) as executor:
        futures = [(cell,executor.submit(run,cell)) for cell in cases]
        for cell,future in futures:
            try:
                future.result()
            except Exception as exc:
                errors.append({"cellule":cell,"raison":str(exc)})
                print(f"ÉCHEC {cell}: {exc}", flush=True)
    if errors:
        dump(b.dir/f"echecs-{time.time_ns()}.json",errors)
        raise SystemExit(1)


if __name__ == "__main__":
    main()
