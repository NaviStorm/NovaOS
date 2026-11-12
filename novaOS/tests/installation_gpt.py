"""Installer et redémarrer uniquement des disques créés par ce banc dans un dossier neuf."""
import argparse
import json
import re
import shutil
from pathlib import Path
import socket
import struct
import subprocess
import tempfile
import time
import uuid
import zlib

from dma_sequentiel import PROMPT, SERIE, compteurs, etat_pci
from installation_utm import FIRMWARE, ROOT, UTM, empreinte, lanceurs
from profils_utm import disque_sequentiel
from prototype_uefi import image, pflash, qmp, tpm

MIO = 1 << 20


def attendre(vm, path, marker, start=0, timeout=180):
    text = b""
    until = time.monotonic() + timeout
    while vm.poll() is None and time.monotonic() < until:
        text = path.read_bytes()[start:] if path.exists() else b""
        if marker in text:
            return text
        if any(m in text for m in (b"bilan Err(", b"QUARANTAINE ", b"Panique du noyau",
                                   b"No mapping found", b"Synchronous Exception")):
            break
        time.sleep(.04)
    raise RuntimeError(f"{marker!r} absent dans {path}\n{text[-900:]!r}")


def verifier_gpt(path, s, ids):
    total = path.stat().st_size // s
    with path.open("rb") as disk:
        def lire(lba, n):
            disk.seek(lba * s)
            return disk.read(n)
        mbr = lire(0,s)
        assert mbr[450] == 0xee and mbr[510:512] == b"\x55\xaa"
        assert struct.unpack_from("<II", mbr,454) == (1,total - 1)
        entries = []
        for lba, other in ((1,total-1),(total-1,1)):
            hdr = bytearray(lire(lba,s))
            assert hdr[:8] == b"EFI PART" and struct.unpack_from("<II",hdr,8) == (65536,92)
            checksum, = struct.unpack_from("<I",hdr,16)
            hdr[16:20] = bytes(4)
            assert zlib.crc32(hdr[:92]) == checksum
            assert struct.unpack_from("<QQ",hdr,24) == (lba,other)
            assert hdr[56:72] == ids[:16]
            entry, n, length, crc = struct.unpack_from("<QIII",hdr,72)
            assert (n,length) == (128,128)
            assert entry == (2 if lba==1 else lba-16384//s)
            raw = lire(entry,n*length)
            assert zlib.crc32(raw)==crc
            entries.append(raw)
        a,b=entries
        assert a == b and a[256:]==bytes(len(a)-256)
        assert a[:16] == uuid.UUID("c12a7328-f81f-11d2-ba4b-00a0c93ec93b").bytes_le
        assert a[128:144] == uuid.UUID("3687f202-9e5a-4df6-83db-17bb308172f9").bytes_le
        assert a[16:32]+a[144:160] == ids[16:]
        esp,last=struct.unpack_from("<QQ",a,32)
        p9,end=struct.unpack_from("<QQ",a,160)
        assert esp < last < p9 < end < total-1-16384//s
        assert all(l*s % MIO == 0 for l in (esp,p9,end+1))
        assert lire(esp,s)[82:90] == b"FAT32   "
        assert lire(p9,8) == lire(p9+4096//s,8) == b"NOVAP0FS"
        return dict(esp=esp,p9=p9,fin_p9=end,taille_secteur=s)


def machine(tmp, dossier, nom, source, cible, secteur, guids, action, *, doublon=False, coupure=False):
    console = tmp / "uart"
    serie = dossier / (nom+".serial")
    trace = dossier / (nom+".trace")
    cmd = [tmp/"qemu-aarch64-softmmu", "-L", FIRMWARE, "-machine", "virt-10.0,gic-version=3,its=off",
           "-accel","tcg","-cpu","cortex-a72","-smp","1","-m","256","-nic","none","-display","none",
           "-device","ramfb","-monitor","none","-qmp","stdio","-S","-rtc","base=utc,clock=host",
           "-chardev",f"socket,id=uart,path={console},server=on,wait=off,logfile={serie}","-serial","chardev:uart",
           "-drive",f"if=pflash,unit=0,format=raw,readonly=on,file={dossier/'code.fd'}",
           "-drive",f"if=pflash,unit=1,format=raw,file={dossier/'vars.fd'}",
           "-device","virtio-iommu-pci,addr=5,granule=4k,aw-bits=48,boot-bypass=off"]
    if source:
        cmd += ["-fw_cfg",f"name=opt/novaos/gpt-guids,file={guids}",
                "-drive",f"if=none,id=source,format=raw,readonly=on,file={source}"]
        cmd += disque_sequentiel("source",6,True)
    backend = f"blkdebug::{cible}" if coupure else cible
    cmd += ["-drive",f"if=none,id=cible,format=raw,file={backend},rerror=report,werror=report"]
    role = "NovaOS-Install-v1" if doublon else ("NovaOS-Target-v1" if source else "NovaOS-System-v1")
    dev = disque_sequentiel("cible",7,False,designation=role)
    dev[1] += f",logical_block_size={secteur},physical_block_size={secteur}"
    if not source:
        dev[1] += ",bootindex=1"
    cmd += dev
    for event in ("virtio_iommu_attach","virtio_iommu_detach","virtio_iommu_map","virtio_iommu_unmap_done",
                  "virtio_iommu_report_fault","virtio_blk_handle_read","virtio_blk_handle_write"):
        cmd += ["-trace",event]
    with tpm(tmp,dossier,nom,dossier/"tpm") as sock, trace.open("xb") as log:
        cmd += ["-chardev",f"socket,id=tch,path={sock}","-tpmdev","emulator,id=tpm0,chardev=tch",
                "-device","tpm-crb-device,tpmdev=tpm0"]
        with subprocess.Popen(cmd,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=log,bufsize=0) as vm:
            try:
                qmp(vm,"qmp_capabilities")
                assert not etat_pci(vm,7)
                with socket.socket(socket.AF_UNIX) as canal:
                    canal.connect(str(console))
                    qmp(vm,"cont")
                    stats=action(vm,canal,serie,trace)
                    stats["rss_emulateur_kio_au_jalon"] = int(subprocess.check_output(["ps","-o","rss=","-p",str(vm.pid)]))
                    if coupure:
                        vm.kill()
                    else:
                        qmp(vm,"quit")
                vm.wait(timeout=10)
            finally:
                if vm.poll() is None:
                    vm.kill()
                vm.wait()
                console.unlink(missing_ok=True)
    return stats


def controle_domaines(trace, *, installation, secteur, volume):
    domaine = None
    plages = set()
    attaches = []
    ecritures = {1: 0, 2: 0}
    for line in trace.read_text().splitlines():
        if line.startswith("virtio_iommu_attach "):
            assert domaine is None and not plages
            domaine, sid = map(int, re.findall(r"=(\d+)", line))
            attaches.append((domaine, sid))
        elif line.startswith("virtio_iommu_detach "):
            assert not plages and f"domain={domaine} " in line
            domaine = None
        elif line.startswith(("virtio_iommu_map ", "virtio_iommu_unmap_done ")):
            assert f"domain={domaine} " in line
            plage = tuple(re.findall(r"virt_(?:start|end)=(0x\w+)", line))
            assert len(plage) == 2
            if line.startswith("virtio_iommu_map "):
                assert plage not in plages
                plages.add(plage)
            else:
                plages.remove(plage)
        elif line.startswith("virtio_blk_handle_write "):
            assert domaine == 1 and plages
            lba,n = map(int, re.search(r" sector (\d+) nsectors (\d+)",line).groups())
            if not installation:
                assert volume["p9"]*secteur <= lba*512
                assert (lba+n)*512 <= (volume["fin_p9"]+1)*secteur
            ecritures[domaine] += 1
        elif line.startswith("virtio_blk_handle_read "):
            assert domaine in (1,2) and plages
        assert not line.startswith("virtio_iommu_report_fault ")
    assert not plages and domaine is None
    assert attaches == [(2,48 if installation else 56),(1,56)]
    return dict(attaches=attaches, ecritures_uefi=ecritures[2], ecritures_noyau=ecritures[1], plages_finales=0)


def securite(tmp, dossier, source, cible, secteur, guids, bilan, sauver):
    resultats = bilan["securite"] = {}

    def essai(nom, disque, support, action, **options):
        resultats[nom] = machine(tmp,dossier,nom,support,disque,secteur,guids,action,**options)
        sauver()

    def refus(vm, canal, serie, trace):
        attendre(vm,serie,b"CIBLE non vierge octet=")
        attendre(vm,serie,b"INSTALLATION bilan Err(Device)")
        assert b"confirmation requise" not in serie.read_bytes()
        fin = compteurs(vm)
        assert all(v["wr_bytes"] == v["wr_operations"] == 0 for v in fin.values())
        assert b"virt_start=0x8006000 " not in trace.read_bytes()
        return dict(fin=fin)

    def ambigu(vm, canal, serie, trace):
        attendre(vm,serie,b"No mapping found")
        fin = compteurs(vm)
        assert all(v["rd_bytes"] == v["wr_bytes"] == 0 for v in fin.values())
        assert not etat_pci(vm,6) and not etat_pci(vm,7)
        assert b"virtio_iommu_attach " not in trace.read_bytes()
        return dict(fin=fin)

    avant = empreinte(cible)
    essai("non-vierge",cible,source,refus)
    essai("ambigu",cible,source,ambigu,doublon=True)
    assert empreinte(cible) == avant

    def suspension(vm, commande):
        retour = qmp(vm,"human-monitor-command",**{"command-line":f'qemu-io cible "{commande}"'})
        if retour:
            raise RuntimeError(retour)

    def couper_ecriture(vm, canal, serie, trace):
        attendre(vm,serie,b"nova> ")
        suspension(vm,"break write_aio publication")
        debut = trace.stat().st_size
        canal.sendall(b"cp /systeme /nouveau\r")
        attendre(vm,trace,b"virtio_blk_handle_write ",debut)
        suspension(vm,"wait_break publication")
        return dict(frontiere="première écriture de cp suspendue",fin=compteurs(vm))

    def relire(vm, canal, serie, trace):
        attendre(vm,serie,b"nova> ")
        start = serie.stat().st_size
        canal.sendall(b"cat /persistant\r")
        attendre(vm,serie,b"NovaOS\r\n",start)
        canal.sendall(b"exit\r")
        attendre(vm,serie,b"SYSTEME bilan Ok(())")
        return dict(fin=compteurs(vm))

    copie = dossier/"coupure-p9.raw"
    with cible.open("rb") as src, copie.open("xb") as dst:
        shutil.copyfileobj(src,dst)
    essai("coupure-ecriture",copie,None,couper_ecriture,coupure=True)
    essai("reprise-ecriture",copie,None,relire)
    verifier_gpt(copie,secteur,guids.read_bytes())

    incomplet = dossier/"coupure-installation.raw"
    taille = cible.stat().st_size
    with incomplet.open("xb") as dst:
        dst.truncate(taille)

    def annuler(vm,canal,serie,trace):
        attendre(vm,serie,PROMPT.encode())
        canal.sendall(b"NON\r")
        attendre(vm,serie,b"INSTALLATION bilan Err(State)")
        stats=compteurs(vm)
        assert all(v["wr_bytes"] == v["wr_operations"] == 0 for v in stats.values())
        return dict(fin=stats)

    essai("sans-consentement",incomplet,source,annuler)
    def couper_installation(vm,canal,serie,trace):
        attendre(vm,serie,PROMPT.encode())
        suspension(vm,"break flush_to_disk gpt")
        canal.sendall(f"AUTORISER {SERIE} {taille}\r".encode())
        attendre(vm,serie,b"INSTALLATION ecriture autorisee")
        limite = time.monotonic() + 20
        while compteurs(vm)["cible"]["wr_bytes"] < 16384 + secteur:
            if time.monotonic() > limite:
                raise RuntimeError("copie GPT secondaire absente")
            time.sleep(.02)
        suspension(vm,"wait_break gpt")
        stats=compteurs(vm)
        assert stats["cible"]["wr_bytes"] == 16384+secteur
        return dict(frontiere="premier FLUSH GPT suspendu",fin=stats)

    essai("coupure-installation",incomplet,source,couper_installation,coupure=True)
    with incomplet.open("rb") as disk:
        debut=disk.read(2*secteur)
        disk.seek(-secteur,2)
        copie_gpt = disk.read(secteur)
    etat=dict(mbr=debut[510:512]==b"\x55\xaa",primaire=debut[secteur:secteur+8]==b"EFI PART",
              secondaire=copie_gpt[:8]==b"EFI PART")
    resultats["etat_apres_coupure"] = etat
    sauver()
    avant = empreinte(incomplet)
    essai("refus-apres-coupure",incomplet,source,refus)
    assert empreinte(incomplet) == avant
    if etat["primaire"] != etat["secondaire"]:
        raise RuntimeError("Arrêt de qualification : GPT primaire et secondaire divergentes après coupure")


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("fv",type=Path)
    p.add_argument("installateur",type=Path)
    p.add_argument("sortie",type=Path)
    p.add_argument("--securite", action="store_true", help="contrôler les refus et les coupures sur des copies jetables")
    p.add_argument("--secteur",type=int,choices=[512,4096],default=512)
    a=p.parse_args()
    dossier=a.sortie.resolve()
    dossier.mkdir()
    (dossier/"tpm").mkdir()
    bilan={}
    def sauver():
        (dossier/"resultats.json").write_text(json.dumps(bilan,indent=2)+"\n")
    with tempfile.TemporaryDirectory(prefix="nvgpt-",dir="/tmp") as temp:
        tmp=Path(temp)
        lanceurs(tmp)
        version=subprocess.check_output([tmp/"qemu-aarch64-softmmu","--version"],text=True).splitlines()[0]
        assert version=="QEMU emulator version 10.0.12"
        assert subprocess.check_output(["plutil","-extract","CFBundleShortVersionString","raw",UTM/"Info.plist"],text=True).strip()=="5.0.8"
        pflash(a.fv/"QEMU_EFI.fd",dossier/"code.fd")
        pflash(a.fv/"QEMU_VARS.fd",dossier/"vars.fd")
        source=image(tmp/"support",a.installateur,"installer-gpt")
        bilan.update(firmware=empreinte(a.fv/"QEMU_EFI.fd"),installateur=empreinte(a.installateur),support=empreinte(source))
        guids=dossier/"guids.bin"
        guids.write_bytes(b"".join(uuid.uuid4().bytes_le for _ in range(3)))
        cible=dossier/"cible.raw"
        taille=(64 if a.secteur==512 else 268)*MIO
        with cible.open("xb") as f:
            f.truncate(taille)
        def installer(vm,canal,serie,trace):
            attendre(vm,serie,PROMPT.encode())
            avant=compteurs(vm)
            assert avant["cible"]["rd_bytes"]==taille
            assert all(v["wr_bytes"]==v["wr_operations"]==0 for v in avant.values())
            assert not etat_pci(vm,6)
            assert b"virt_start=0x8006000 " not in trace.read_bytes()
            canal.sendall(f"AUTORISER {SERIE} {taille}\r".encode())
            attendre(vm,serie,b"INSTALLATION bilan Ok(())",timeout=900)
            fin=compteurs(vm)
            assert fin["cible"]["wr_bytes"] and fin["source"]["wr_bytes"]==0
            assert not etat_pci(vm,7)
            return dict(avant=avant,fin=fin)
        bilan["installation"]=machine(tmp,dossier,"installation",source,cible,a.secteur,guids,installer)
        assert empreinte(source)==bilan["support"]
        bilan["gpt"]=verifier_gpt(cible,a.secteur,guids.read_bytes())
        bilan["installation"]["dma"] = controle_domaines(dossier/"installation.trace", installation=True, secteur=a.secteur, volume=bilan["gpt"])
        sauver()
        for nom in ("autonome","persistance"):
            def shell(vm,canal,serie,trace):
                attendre(vm,serie,b"nova> ")
                def commande(ligne,retour):
                    start=serie.stat().st_size
                    canal.sendall(ligne.encode()+b"\r")
                    texte=attendre(vm,serie,retour,start)
                    assert b"indisponible" not in texte
                if nom=="autonome":
                    commande("cp /systeme /persistant",b"nova> ")
                commande("cat /persistant",b"NovaOS\r\n")
                commande("exit",b"SYSTEME bilan Ok(())")
                return dict(fin=compteurs(vm))
            bilan[nom]=machine(tmp,dossier,nom,None,cible,a.secteur,guids,shell)
            bilan[nom]["dma"] = controle_domaines(dossier/(nom+".trace"), installation=False, secteur=a.secteur, volume=bilan["gpt"])
            sauver()
        if a.securite:
            securite(tmp, dossier, source, cible, a.secteur, guids, bilan, sauver)


if __name__=="__main__":
    main()
