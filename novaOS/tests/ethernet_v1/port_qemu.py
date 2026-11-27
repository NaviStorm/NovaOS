"""Qualification Ethernet v1 sur les deux machines du témoin VirtIO-net audité."""
import argparse
from contextlib import ExitStack, contextmanager
from datetime import datetime, timezone
from hashlib import sha256
from functools import lru_cache
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import resource
import shutil
import signal
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time
import uuid

NOVA = Path(__file__).resolve().parents[2]
AUDIT = "56db7d5c0d04490cd284b91567cbbd4736534266"


PERF = {mode + str(size): 12 + group * 2 + i
        for group, mode in enumerate(("echo", "tx", "rx", "duplex"))
        for i, size in enumerate((60, 1514))}
PERF_FRAMES = 13 * 1024

@lru_cache(maxsize=1024)
def motif(size, low, retour):
    return bytes(((29 * low + 17 * i) & 255) ^ (165 if retour else 0) for i in range(size - 15))

def dump(path, data):
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n")


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


@contextmanager
def qualification(worktrees, report):
    lock=worktrees/".qualification-utilitaires-exclusive.lock"
    marker=lock/"owner.json"
    owner=dict(pid=os.getpid(),token=uuid.uuid4().hex,worktree=str(NOVA.parent),action="port Ethernet v1")
    def utc():
        return datetime.now(timezone.utc).isoformat()
    start=time.monotonic()
    report["debut_attente"]=utc()
    while True:
        try:
            lock.mkdir()
            break
        except FileExistsError:
            time.sleep(2)
    try:
        dump(marker, owner)
        report.update(acquisition=utc(),attente_s=time.monotonic()-start)
        processus=subprocess.check_output(["ps","-axo","comm="],text=True).splitlines()
        if any("qemu-system-" in proc or "hyperfine" in proc for proc in processus):
            raise RuntimeError("Charge incompatible avec le verrou partagé")
        yield
    finally:
        if marker.is_file() and json.loads(marker.read_text())==owner:
            marker.unlink()
            lock.rmdir()
            report["liberation"]=utc()


def vtd_trafic(out, cycles=1):
    """Relecture des permissions réellement utilisées, entre les transitions de BME."""
    bme=False
    ouvertures=0
    physical={}
    traductions=0
    with (out/"qemu.trace").open() as lines:
        for line in lines:
            if line.startswith("vtd_dmar_fault "):
                raise ValueError("Faute VT-d")
            cmd=re.match(r"pci_cfg_write virtio-net-pci 00:07\.0 @0x4 <- (0x[0-9a-f]+)$",line)
            if cmd:
                active=bool(int(cmd[1],16)&4)
                if active and not bme:
                    if len({v>>12 for v in physical.values()}) != len(physical):
                        raise ValueError("Cadres VT-d partagés")
                    physical.clear()
                    ouvertures += 1
                bme=active
            if not line.startswith("vtd_iotlb_page_") or "sid 0x38 " not in line:
                continue
            tr=re.search(r"sid 0x38 iova (0x[0-9a-f]+) slpte (0x[0-9a-f]+) domain 0x3$",line)
            if not tr or not bme:
                raise ValueError("Traduction du NIC hors de son bail")
            index=(int(tr[1],16)-0x10000000)//4096
            val=int(tr[2],16)
            if not 0<=index<22 or val&4095!=(2 if index in (2,5,*range(6,14)) else 1) or physical.setdefault(index,val)!=val:
                raise ValueError("Traduction hors des feuilles autorisées")
            traductions+=1
    if bme or ouvertures!=cycles or not traductions or len({v>>12 for v in physical.values()})!=len(physical):
        raise ValueError("Attribution VT-d ou restitution BME incohérente")
    return dict(pages_touchees=len(physical),echantillons=traductions,attributions=ouvertures,bme_final=bme)


class Pair:
    """Vérifie chaque datagramme du socket QEMU et conserve les deux directions en PCAP."""
    def __init__(self, sock, out, ref, cas):
        self.sock = sock
        self.out = out
        self.ref = ref
        self.cas = cas
        self.rx_perf = 0
        self.rx_seul = 0
        self.filtrees = 0
        self.frames = self.octets = 0
        self.first = self.last = 0
        self.groupes_pair = []
        self.error = None
        self.thread = threading.Thread(target=self.run, daemon=True)

    def oracle(self, numero, retour):
        if self.cas in ("cycles", "cycles-256", "recu-arm-differe"):
            numero %= 144
        elif self.cas == "duo-reouverture":
            numero %= 4
        elif self.cas in ("cycles-court", "recu-vtd-differe", "reset-court", "reset-iommu-differe"):
            numero = 0
        if self.cas in ("mixte100k", "historique100k"):
            return self.ref.trame_longue(numero,retour)
        if self.cas not in PERF and self.cas not in ("mesure60","mesure1514","historique1514"):
            fn=self.ref.trame if numero<16 else self.ref.trame_longue
            return fn(numero,retour)
        size=60 if self.cas.endswith("60") else 1514
        frame=bytearray(self.ref.trame(numero%256,retour)[:15])
        frame.extend(motif(size, numero % 256, retour))
        if numero>=16 or self.cas=="historique1514":
            frame[14:18]=numero.to_bytes(4,"big")
        return bytes(frame)

    def run(self):
        try:
            with self.sock.makefile("rb") as stream, gzip.open(self.out/"pair.pcap.gz", "xb", compresslevel=1) as trace:
                trace.write(struct.pack("<IHHIIII", 0xa1b2c3d4, 2, 4, 0, 0, 65535, 1))
                def pcap(frame):
                    stamp=time.time_ns()//1000
                    n=len(frame)
                    trace.write(struct.pack("<IIII",stamp//1000000,stamp%1000000,n,n)+frame)
                if self.cas in ("veille", "filtre"):
                    limit=time.monotonic()+35
                    while not (self.out/"serie.log").exists() or "ETH3 RX attente bloquée" not in (self.out/"serie.log").read_text(errors="replace"):
                        if time.monotonic()>limit: raise TimeoutError("Client en attente introuvable")
                        time.sleep(.01)
                    if self.cas == "filtre":
                        for i in range(16):
                            frame = bytearray(self.ref.trame(i, True))
                            if i % 2:
                                frame[6:12] = bytes.fromhex("024e56000002")
                            else:
                                frame[12:14] = bytes.fromhex("88b6")
                            self.sock.sendall(struct.pack(">I", len(frame)) + frame)
                            pcap(frame)
                            self.filtrees += 1
                    for i in range(16):
                        frame=self.ref.trame(i,True)
                        self.sock.sendall(struct.pack(">I",len(frame))+frame)
                        pcap(frame)
                        self.rx_seul+=1
                def injecter(numero):
                    frame = self.oracle(numero, True)
                    self.sock.sendall(struct.pack(">I", len(frame)) + frame)
                    with verrou_trace:
                        pcap(frame)
                    self.rx_perf += 1
                def generateur():
                    try:
                        path = self.out / "serie.log"
                        while not path.exists():
                            if arret.wait(.005):
                                return
                        with path.open("rb") as serial:
                            pending_line = b""
                            while self.rx_perf < PERF_FRAMES and not arret.is_set():
                                data = serial.read(8192)
                                if not data:
                                    arret.wait(.0005)
                                    continue
                                lines = (pending_line + data).split(b"\n")
                                pending_line = lines.pop()
                                for line in lines:
                                    match = re.search(rb"E3B admission_rx (1|4|8)\s*$",line)
                                    if self.cas == "rattrapage" and b"E3C dernier lot RX generation=" in line:
                                        for i in range(16):
                                            injecter(i)
                                    elif match:
                                        for _ in range(int(match[1])):
                                            injecter(self.rx_perf)
                    except Exception as err:
                        self.error = repr(err)
                verrou_trace = threading.Lock()
                arret = threading.Event()
                generateur_rx = None
                if self.cas.startswith(("rx", "duplex")) or self.cas == "rattrapage":
                    generateur_rx = threading.Thread(target=generateur, daemon=True)
                    generateur_rx.start()
                pending=[]
                while header := stream.read(4):
                    if len(header) != 4:
                        raise ValueError("En-tête socket tronqué")
                    n, = struct.unpack(">I", header)
                    if n not in (60, 1514):
                        raise ValueError(f"Longueur de trame {n}")
                    data = stream.read(n)
                    oracle = self.oracle
                    if data != oracle(self.frames, False):
                        raise ValueError(f"Trame TX {self.frames} différente du témoin")
                    reply = oracle(self.frames, True)
                    with verrou_trace:
                        pcap(data)
                    if not self.cas.startswith(("duplex", "tx", "rx")):
                        pending.append(reply)
                    if self.cas!="temoin16" or len(pending)==16:
                        for frame in pending:
                            self.sock.sendall(struct.pack(">I",len(frame))+frame)
                            pcap(frame)
                        pending.clear()
                    now = time.monotonic_ns()
                    if not self.frames:
                        self.first = now
                    self.frames += 1
                    self.octets += n
                    self.last = now
                    if self.cas in ("mixte100k", "historique100k") and self.frames % 16 == 0:
                        self.groupes_pair.append(now)
                    if self.cas.startswith("tx") and self.frames == PERF_FRAMES:
                        for i in range(16):
                            injecter(i)
                arret.set()
                if generateur_rx:
                    generateur_rx.join(2)
        except Exception as err:
            self.error = repr(err)


def main():
    import cycle_vie
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch", choices=("arm", "q35"))
    p.add_argument("cas", choices=("boite", "data", "deces", "incertain", "endurance", "revocation", "veille", "filtre", "court", "temoin16", "port","plein","mesure60","mesure1514", "mixte100k","historique100k","historique1514","ancien","choix-data","segment-interrompu","recu-collecte", *cycle_vie.NOUVEAUX, *PERF))
    for name in ("elf", "sortie", "temoin", "archive", "limine", "relais"):
        p.add_argument(name, type=lambda v: Path(v).resolve())
    p.add_argument("--cycle-vie", action="store_true", help="Vérifier les reçus et la mémoire du profil 3C")
    p.add_argument("--source-elf", help="Révision de construction attestée par le manifeste SHA256 voisin")
    p.add_argument("--delai", type=int, default=90)
    p.add_argument("--cpu-max", action="store_true", help="Témoin ArmVirt avec RNDR et chargeur direct")
    p.add_argument("--limine-arm", type=Path, help="Second étage Limine de laboratoire identifié")
    a = p.parse_args()
    sys.dont_write_bytecode = True
    if subprocess.check_output(["git", "-C", a.temoin, "rev-parse", "HEAD"], text=True).strip() != AUDIT:
        raise ValueError("Révision du témoin inattendue")
    sys.path[:0] = [str(NOVA/"scripts"), str(a.temoin/"novaOS/tests"), str(a.temoin/"novaOS/scripts")]
    from esp_installation import construire
    from virtio_net_dxe import firmware_files
    from virtio_net_relais_q35 import support, verifier_tables
    from virtio_net_confinement import capture
    from virtio_net_domaines_arm import verifier as verifier_domaines
    from installation_utm import lanceurs
    from prototype_uefi import tpm
    from bloc import qmp
    import virtio_net_pair
    a.sortie.mkdir(parents=True, exist_ok=False)
    out = a.sortie
    shutil.copyfile(a.elf,out/"novaos.elf")
    if a.source_elf:
        manifest=json.loads((a.elf.parent/"construction.json").read_text())
        if manifest["commit"] != a.source_elf or manifest["elf"][a.arch] != digest(out/"novaos.elf"):
            raise ValueError("Empreinte ELF ou révision incohérente")
    a.elf=out/"novaos.elf"
    refdir = a.archive / "cloture-20261126" / f"confirmation-{a.arch}-endurance"
    args = json.loads((refdir/"arguments.json").read_text())
    report = dict(audit=AUDIT, arch=a.arch, cas=a.cas, elf=digest(a.elf), modele=digest(Path(args[0])), succes=False)
    report["sha256_lanceur"]=digest(Path(__file__))
    report["commit_sources"]=subprocess.check_output(["git","-C",NOVA,"rev-parse","HEAD"],text=True).strip()
    report["sources_suivies_modifiees"]=bool(subprocess.check_output(["git","-C",NOVA,"status","--porcelain","--untracked-files=no"]))
    if a.cas == "historique1514":
        index = a.temoin / "novaOS/docs/validation/virtio-net/finale-20261125/ARTEFACTS-LOCAUX-SHA256"
        nom = f"{a.arch}-l2-12.elf"
        if [line.split()[0] for line in index.read_text().splitlines() if line.split()[1]==nom] != [report["elf"]]:
            raise ValueError("ELF de performance différent de l'archive historique")
        report["commit_lanceur"] = report["commit_sources"]
        report["commit_sources"] = None
        report["provenance_elf"] = str(index.relative_to(a.temoin))+"#"+nom
    if a.cas == "historique100k":
        provenance = a.temoin / "novaOS/docs/validation/virtio-net/cloture-20261126/applicabilite-candidat-final.json"
        source = json.loads(provenance.read_text())
        assert report["elf"] == source["comparaisons"][a.arch+"-endurance"]["ancien"]["sha256"]
        report["commit_lanceur"] = report["commit_sources"]
        report["commit_sources"] = source["reference_executions"]
        report["provenance_elf"] = str(provenance.relative_to(a.temoin))
    if a.source_elf:
        report["commit_lanceur"] = report["commit_sources"]
        report["commit_sources"] = a.source_elf
        report["construction"] = manifest
    try:
        with ExitStack() as stack:
            stack.enter_context(qualification(Path(os.environ["NOVAOS_TMPDIR"]).resolve().parent, report))
            tmp = Path(stack.enter_context(tempfile.TemporaryDirectory(prefix="e3", dir=os.environ["NOVAOS_TMPDIR"])))
            if a.arch == "arm":
                lanceurs(tmp)
                report["distribution"] = firmware_files(tmp)
                disk = tmp/"systeme.raw"
                shutil.copyfile(refdir/"systeme.raw", disk)
                with disk.open("r+b") as fp:
                    fp.seek(1024*1024)
                    if fp.read(3) != b"\xeb\x58\x90":
                        raise ValueError("ESP historique introuvable")
                    fat = construire(a.elf, 512, scenario="net-arm-p9", relais_boot=None if a.cpu_max or a.cas=='temoin16' else a.relais, chargeur_labo=a.limine_arm)
                    for i in range(0, len(fat), 516):
                        sector, = struct.unpack_from("<I", fat, i)
                        fp.seek(1024*1024 + sector*512)
                        fp.write(fat[i+4:i+516])
                tpm_socket = stack.enter_context(tpm(tmp,out,"labo"))
                report["firmware"] = digest(tmp/"code.fd")
                report["limine_arm"] = digest(a.limine_arm or NOVA/"third_party/limine/BOOTAA64.EFI")
                report["relais"] = None if a.cpu_max or a.cas=="temoin16" else digest(a.relais)
                if a.cpu_max:
                    args[args.index("-cpu")+1]="max"
            else:
                firmware = a.archive/"ovmf-q35-01/variante"
                support(a.elf, firmware, tmp, a.limine, ethernet=True)
                report["entrees"] = json.loads((tmp/"entrees.json").read_text())
                disk = tmp/"support.raw"
            host, guest = socket.socketpair()
            stack.callback(host.close)
            stack.callback(guest.close)
            for i, value in enumerate(args):
                if value == "-netdev":
                    args[i+1] = re.sub(r"fd=\d+", f"fd={guest.fileno()}", args[i+1])
                elif a.arch == "arm" and a.cas == "reset-sans-option" and value == "-device" and args[i+1].startswith("virtio-net-pci,"):
                    args[i+1] += ",queue_reset=off"
                elif value in ("-serial", "-debugcon"):
                    args[i+1] = "file:" + str(out/Path(args[i+1]).name)
                elif value == "-drive":
                    dev = args[i+1]
                    if "id=source," in dev:
                        args[i+1] = re.sub(r"file=[^,]+", f"file={disk}", dev)
                    elif "unit=1," in dev or ("unit=0," in dev and a.arch=="arm"):
                        args[i+1] = re.sub(r"file=([^,]+)", lambda m: "file="+str(tmp/Path(m[1]).name), dev)
                elif a.cas not in ("temoin16","historique100k","historique1514") and value == "-fw_cfg" and "opt/novaos/net-p9" in args[i+1]:
                    args[i+1]="name=opt/novaos/net-p9,string=ethernet-p9"
                elif value == "-chardev" and "id=tch," in args[i+1]:
                    args[i+1] = f"socket,id=tch,path={tpm_socket}"
            if a.cas=="endurance":
                for event in ("virtio_iommu_translate","vtd_iotlb_page_hit","vtd_dmar_translate"):
                    if event in args:
                        j=args.index(event)
                        del args[j-1:j+1]
            dump(out/"arguments.json",args)
            peer = Pair(host,out,virtio_net_pair,a.cas)
            with (out/"qemu.trace").open("xb") as log:
                debut_cpu=resource.getrusage(resource.RUSAGE_CHILDREN)
                debut_vm=time.monotonic()
                with subprocess.Popen(args, cwd=tmp, pass_fds=(guest.fileno(),), stdin=subprocess.PIPE,
                                      stdout=subprocess.PIPE, stderr=log, bufsize=0) as vm:
                    guest.close()
                    peer.thread.start()
                    try:
                        qmp(vm,"qmp_capabilities")
                        report["qemu"]=qmp(vm,"query-version")
                        capture(vm,out,"reset",a.arch=="arm")
                        qmp(vm,"cont")
                        limit=time.monotonic()+a.delai
                        while time.monotonic()<limit and vm.poll() is None:
                            serial=(out/"serie.log").read_text(errors="replace")
                            if any(mark in serial for mark in ("NET Q35 etat=", "NET DOMAINE bilan", "NET ARM bilan", "ETH3 refus ouverture", "Panique du noyau", "Exception noyau", "Synchronous Exception")):
                                break
                            if peer.error:
                                break
                            time.sleep(.1)
                        qmp(vm,"stop")
                        final=capture(vm,out,"fin",a.arch=="arm")
                        if a.arch=="arm" and (a.cas=="court" or a.cas.startswith("reset-")) and a.cycle_vie:
                            report["capacite_pci_arm"]=cycle_vie.capacite_pci_arm(vm, serial)
                        if final["pci_command"] & 4:
                            raise ValueError("BME du NIC toujours actif à la fin")
                        if a.arch=="q35" and "NET Q35 etat=" in serial:
                            with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as qt:
                                qt.connect(str(tmp/"qtest.sock"))
                                with qt.makefile("rwb",buffering=0) as stream:
                                    retire = "NET Q35 retrait cookie=" in serial
                                    if a.cycle_vie:
                                        report["preuve_qi"] = cycle_vie.preuve_qi(vm,stream,serial,retire or a.cas=="recu-vtd-absent")
                                    if not (a.cycle_vie and a.cas=="recu-vtd-absent"):
                                        cycles = cycle_vie.nombre_cycles(a.cas,"q35") if retire and a.cycle_vie else 1
                                        report["cadres_vtd_controles"]=verifier_tables(vm,stream,serial.encode(),out,retire,cycles)
                        if "Synchronous Exception" in serial:
                            report["registres_efi"]=qmp(vm,"human-monitor-command",**{"command-line":"info registers"})
                            report["code_efi"]=qmp(vm,"human-monitor-command",**{"command-line":"xp /12i 0x4e936730"})
                        qmp(vm,"quit")
                        vm.wait(timeout=5)
                    finally:
                        if vm.poll() is None:
                            vm.kill()
                            vm.wait()
                        fin_cpu=resource.getrusage(resource.RUSAGE_CHILDREN)
                        report["cpu_qemu"]=dict(utilisateur_s=fin_cpu.ru_utime-debut_cpu.ru_utime,systeme_s=fin_cpu.ru_stime-debut_cpu.ru_stime,reel_s=time.monotonic()-debut_vm)
                        peer.thread.join(3)
                        report.update(rx_injectees_perf=peer.rx_perf, rx_sans_tx=peer.rx_seul, rx_filtrees=peer.filtrees, trames_par_sens=peer.frames, octets_par_sens=peer.octets, intervalle_pair_ns=peer.last-peer.first, erreur_pair=peer.error)
            serial=(out/"serie.log").read_text(errors="replace")
            if a.arch=="q35" and "NET Q35 etat=" in serial:
                report["dma"]=vtd_trafic(out, cycle_vie.nombre_cycles(a.cas,"q35") if a.cycle_vie else 1)
            if a.arch=="arm":
                if "NET ARM bilan" in serial and a.cas!="endurance" and not a.cycle_vie:
                    report["domaines"]=verifier_domaines(out,True,ethernet=True,quarantaine="quarantaine disque_lisible=true" in serial)
                volume=refdir/"volume-relu.p9"
                with disk.open("rb") as fp:
                    fp.seek(61<<20)
                    report["volume_relu_identique"]=sha256(fp.read(volume.stat().st_size)).hexdigest()==digest(volume)
            report["succes"]=(peer.frames==(65552 if a.cas=="endurance" else 4608 if a.cas in ("mesure60","mesure1514") else 144) and not peer.error and
                              "materiel=Ok(()) retrait=Ok(" in serial and "acteurs=[Exit(0), Exit(0), Exit(0)]" in serial and
                              ("NET Q35 etat=Ok(())" in serial if a.arch=="q35" else report["volume_relu_identique"] and "disque_lisible=true" in serial))
            if a.cas in ("veille", "filtre"):
                attendues = 16 if a.cas == "filtre" else 0
                report["succes"]=bool(peer.filtrees==attendues and peer.rx_seul==16 and peer.frames==0 and not peer.error and "ETH3 RX attente bloquée" in serial and
                    f"ETH3 drainage tx=0 rx={16 + attendues} " in serial and "acteurs=[Exit(0), Exit(0), Exit(0)]" in serial and
                    ("NET Q35 etat=Ok(())" in serial if a.arch=="q35" else "NET ARM bilan Ok(())" in serial))
            if a.cas=="court":
                report["succes"]=bool(peer.frames==1 and not peer.error and "retrait=Err(12)" in serial and
                    "acteurs=[Exit(12), Exit(12), Exit(0)]" in serial and
                    ("NET Q35 L2 quarantaine=" in serial if a.arch=="q35" else report["volume_relu_identique"] and "quarantaine disque_lisible=true" in serial))
            if a.cas=="temoin16":
                report["succes"]=bool(peer.frames==16 and not peer.error and "NET L2 completions TX=16 RX=16" in serial and
                    "ETH3 " not in serial and ("NET Q35 etat=Ok(())" in serial if a.arch=="q35" else "NET ARM bilan Ok(())" in serial))
            if a.cas in ("deces","incertain","revocation"):
                fin_acteurs=re.search(r"acteurs=\[Exit\((\d+)\), Exit\((\d+)\), Exit\((\d+)\)\]",serial)
                report["succes"]=bool(peer.frames==4 and not peer.error and fin_acteurs and
                    fin_acteurs[1] == {"deces":"9","incertain":"11","revocation":"10"}[a.cas] and
                    fin_acteurs.groups()[1:]==("0","0") and "retrait=Err(" in serial and
                    ("NET Q35 L2 quarantaine=" in serial if a.arch=="q35" else report["volume_relu_identique"] and "quarantaine disque_lisible=true" in serial))
            modes={"boite":1,"data":2,"deces":3,"incertain":4,"endurance":5,"revocation":6,"veille":7,"filtre":7,"court":8,"port":0,"plein":9,"mesure60":10,"mesure1514":11}
            if a.cas=="historique1514":
                report["succes"] = (peer.frames==500000 and not peer.error and
                    "NET L2 total TX=500000 RX=500000" in serial and "NET L2 pilote fin=Exit(0) racines=0" in serial and
                    ("NET Q35 etat=Ok(())" in serial if a.arch=="q35" else report["volume_relu_identique"] and "NET ARM bilan Ok(())" in serial))
            if a.cas in ("mixte100k","historique100k"):
                # Même pair, même horloge et mêmes numéros de trames pour les deux ELF.
                records = peer.groupes_pair[15:-16]
                delta = sorted(b-a for a,b in zip(records,records[1:]))
                dump(out/"groupes-pair-ns.json",peer.groupes_pair)
                if delta:
                    report["fenetre_pair"] = dict(premiere=256, derniere=256+16*len(delta),
                        trames=16*len(delta), duree_ns=records[-1]-records[0],
                        p99_intervalles_16_ns=delta[(99*len(delta)+99)//100-1])
                fin = ("NET L2 total TX=100000 RX=100000" in serial and
                       "NET L2 pilote fin=Exit(0) racines=0" in serial) if a.cas=="historique100k" else (
                    "ETH3 drainage tx=100000 rx=100000" in serial and "acteurs=[Exit(0), Exit(0), Exit(0)]" in serial)
                report["succes"] = (len(peer.groupes_pair)==6250 and not peer.error and fin and
                    ("NET Q35 etat=Ok(())" in serial if a.arch=="q35" else report["volume_relu_identique"] and "NET ARM bilan Ok(())" in serial))
            modes.update(PERF)
            modes.update(mixte100k=20, ancien=21)
            modes["choix-data"]=22
            modes.update({"segment-interrompu":23,"recu-collecte":24})
            if a.cas in ("segment-interrompu","recu-collecte"):
                segment=a.cas=="segment-interrompu"
                acteurs=re.search(r"acteurs=\[Exit\((\d+)\), Exit\((\d+)\), Exit\((\d+)\)\]",serial)
                preuve=(acteurs and acteurs[3]=="9" and "appels_pilote=1 " in serial) if segment else (
                    acteurs and acteurs.groups()==("9","0","0"))
                report["succes"]=bool(preuve and not peer.error and peer.frames==(0 if segment else 8)
                    and "retrait=Err(" in serial and ("NET Q35 L2 quarantaine=" in serial if a.arch=="q35" else
                    report["volume_relu_identique"] and "quarantaine disque_lisible=true" in serial))
            if a.cas in PERF:
                tx = 0 if a.cas.startswith("rx") else PERF_FRAMES
                rx = 16 if a.cas.startswith("tx") else PERF_FRAMES
                injectees = 0 if a.cas.startswith("echo") else rx
                report.update(trames_tx=peer.frames,trames_rx=(peer.frames if a.cas.startswith("echo") else peer.rx_perf),
                              rx_hors_mesure=16 if a.cas.startswith("tx") else 0)
                report["succes"] = (peer.frames == tx and peer.rx_perf == injectees and not peer.error
                    and f"ETH3 drainage tx={tx} rx={rx} " in serial
                    and "materiel=Ok(()) retrait=Ok(" in serial and "acteurs=[Exit(0), Exit(0), Exit(0)]" in serial
                    and all(f"ETH3 lot={b} n=1024 " in serial for b in (1,4,8))
                    and ("NET Q35 etat=Ok(())" in serial if a.arch=="q35" else report["volume_relu_identique"] and "NET ARM bilan Ok(())" in serial))
            if a.cycle_vie:
                if a.cas == "echo1514" and not report["succes"]:
                    raise ValueError("Critères du lanceur de performance 3B")
                report["cycle_vie"] = cycle_vie.verifier(out, report, serial)
                report["succes"] = True
                modes.update(cycle_vie.CAS)
            selection=re.search(r"E3B transport_client=Some\((\d+)\)",serial)
            if selection:
                report["transport_client"]=int(selection[1])
                attendu=1 if a.cas in ("data","plein","choix-data") else 2 if a.cas=="ancien" else 3
                report["succes"] &= report["transport_client"]==attendu
            if a.cas in modes:
                details=f"mode={modes[a.cas]} racines=[0, 4, 1] boites_globales={0 if a.cas=='data' else 8 if a.cas=='plein' else 2} endpoints_globaux={13 if a.cas=='plein' else 5}"
                report["profil_verifie"]=details in serial
                report["succes"] &= report["profil_verifie"]
                if a.cas == "boite":
                    report["neuvieme_refuse"] = "ETH3 préparation supplémentaire refusée par le courtier" in serial
                    report["succes"] &= report["neuvieme_refuse"]
                if a.cas in ("boite","data","endurance","plein","port","mesure60","mesure1514","ancien","choix-data"):
                    counts=re.search(r"publications=\[([0-9, ]+)\]",serial)
                    wrapped=int(a.cas=="endurance")
                    report["publications"]=[int(v) for v in counts[1].split(",")] if counts else []
                    report["succes"] &= len(report["publications"])==8 and min(report["publications"],default=0)>0 and f"wraps_tx={wrapped} wraps_rx={wrapped}" in serial
    finally:
        dump(out/"campagne.json",report)
    if not report["succes"]:
        raise SystemExit("Qualification incomplète, consulter " + str(out))


if __name__=="__main__":
    signal.signal(signal.SIGTERM,lambda *_:sys.exit(143))
    main()
