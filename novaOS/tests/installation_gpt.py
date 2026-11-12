"""Installer et redémarrer uniquement des disques créés par ce banc dans un dossier neuf."""
import argparse
import json
import re
import shutil
from pathlib import Path
import socket
import struct
import subprocess
import sys
import tempfile
import time
import uuid
import zlib

from dma_sequentiel import PROMPT, SERIE, compteurs, etat_pci
from installation_utm import FIRMWARE, ROOT, UTM, empreinte, lanceurs
from profils_utm import disque_sequentiel
from prototype_uefi import image, pflash, qmp, tpm

sys.path.insert(0, str(ROOT / "scripts"))
from verifier_gpt import MIO, verifier_gpt


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


def machine(tmp, dossier, nom, source, cible, secteur, guids, action, *, doublon=False, coupure=False,
            erreur_lecture=False, format_cible="raw", arret_force=False):
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
    if erreur_lecture:
        config = tmp / "lecture.blkdebug"
        config.write_text('[inject-error]\nevent = "read_aio"\nerrno = "5"\nsector = "16"\n')
        backend = f"blkdebug:{config}:{cible}"
    assert format_cible in ("raw", "qcow2")
    options = ",discard=unmap,detect-zeroes=unmap" if format_cible == "qcow2" else ""
    cmd += ["-drive",f"if=none,id=cible,format={format_cible},file={backend},rerror=report,werror=report{options}"]
    role = "NovaOS-Install-v1" if doublon else ("NovaOS-Target-v1" if source else "NovaOS-System-v1")
    dev = disque_sequentiel("cible",7,False,designation=role)
    if secteur is not None:
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
                    if coupure or arret_force:
                        vm.kill()
                    else:
                        qmp(vm,"quit")
                code = vm.wait(timeout=10)
                assert code == (-9 if coupure or arret_force else 0)
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

    taille = cible.stat().st_size

    def ecritures_nulles(vm, trace):
        fin = compteurs(vm)
        assert all(v["wr_bytes"] == v["wr_operations"] == 0 for v in fin.values())
        contenu = trace.read_bytes()
        assert b"virt_start=0x8006000 " not in contenu
        assert b"virtio_blk_handle_write " not in contenu
        assert b"virtio_iommu_report_fault " not in contenu
        return fin

    def conserver(nom, disque, action, **options):
        avant = empreinte(disque)
        essai(nom, disque, source, action, **options)
        apres = empreinte(disque)
        resultats[nom].update(sha256_avant=avant, sha256_apres=apres)
        sauver()
        assert apres == avant, f"écriture sur le disque refusé {nom}"

    def refuser(nom, disque, *etats, **options):
        def action(vm, canal, serie, trace):
            attendre(vm, serie, b"CIBLE serie=")
            canal.sendall(f"AUTORISER {SERIE} {taille}\r".encode())
            texte = attendre(vm, serie, b"INSTALLATION bilan Err(")
            assert b"confirmation requise" not in texte and b"ecriture autorisee" not in texte
            assert any(("DISQUE " + e).encode() in texte for e in etats), texte[-1200:]
            etat = next(e for e in etats if ("DISQUE " + e).encode() in texte)
            return dict(classe=etat, fin=ecritures_nulles(vm, trace))
        conserver(nom, disque, action, **options)

    def ambigu(vm, canal, serie, trace):
        attendre(vm,serie,b"No mapping found")
        fin = compteurs(vm)
        assert all(v["rd_bytes"] == v["wr_bytes"] == 0 for v in fin.values())
        assert not etat_pci(vm,6) and not etat_pci(vm,7)
        assert b"virtio_iommu_attach " not in trace.read_bytes()
        return dict(fin=fin)

    refuser("non-vierge", cible, "installation existante")
    conserver("ambigu", cible, ambigu, doublon=True)

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

    copie = dossier/"coupure-copie-fichier.raw"
    with cible.open("rb") as src, copie.open("xb") as dst:
        shutil.copyfileobj(src,dst)
    essai("coupure-ecriture",copie,None,couper_ecriture,coupure=True)
    essai("reprise-ecriture",copie,None,relire)
    verifier_gpt(copie,secteur,guids.read_bytes())

    incomplet = dossier/"coupure-installation.raw"
    with incomplet.open("xb") as dst:
        dst.truncate(taille)

    def annuler(vm,canal,serie,trace):
        attendre(vm,serie,PROMPT.encode())
        canal.sendall(b"NON\r")
        attendre(vm,serie,b"INSTALLATION bilan Err(State)")
        stats=compteurs(vm)
        assert all(v["wr_bytes"] == v["wr_operations"] == 0 for v in stats.values())
        return dict(fin=stats)

    conserver("sans-consentement", incomplet, annuler)
    refuser("erreur-lecture", incomplet, "indetermine identification ou lecture", erreur_lecture=True)
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
    # L'image positive a déjà passé le décodage indépendant et les quatre CRC.
    suffixe = 16384 + secteur
    with incomplet.open("rb") as disk, cible.open("rb") as reference:
        reste = taille - suffixe
        while reste:
            n = min(reste, MIO)
            assert disk.read(n) == bytes(n)
            reste -= n
        reference.seek(-suffixe, 2)
        secondaire = reference.read()
        assert disk.read() == secondaire
    resultats["etat_apres_coupure"] = dict(primaire=False, mbr=False, secondaire=True,
                                          octets_nuls=taille-suffixe, octets_secondaires=suffixe)
    refuser("refus-apres-coupure", incomplet, "installation interrompue")

    def nouvelle(nom):
        disque = dossier / (nom + ".raw")
        with disque.open("xb") as f:
            f.truncate(taille)
        return disque

    # Chaque fixture est indépendante de la pièce interrompue conservée.
    for nom, position in (("crc-entete", -secteur+16), ("crc-tableau", -suffixe+32),
                          ("donnee-supplementaire", 12345)):
        disque = nouvelle(nom)
        with disque.open("r+b") as f:
            f.seek(-suffixe, 2)
            f.write(secondaire)
            f.seek(position, 2 if position < 0 else 0)
            octet = f.read(1)[0]
            f.seek(-1, 1)
            f.write(bytes([octet ^ 1]))
        refuser(nom, disque, "metadonnees incoherentes")

    disque = nouvelle("origine-inconnue")
    with disque.open("r+b") as f:
        f.seek(taille//2)
        f.write(b"Nova")
    refuser("origine-inconnue", disque, "non vierge origine indeterminee")

    # Une GPT valide avec un GUID de disque différent a une origine inconnue.
    autre = bytearray(secondaire)
    h = memoryview(autre)[-secteur:]
    h[56:72] = uuid.uuid4().bytes_le
    h[16:20] = bytes(4)
    struct.pack_into("<I", h, 16, zlib.crc32(h[:92]))
    disque = nouvelle("gpt-etrangere")
    with disque.open("r+b") as f:
        f.seek(-suffixe, 2)
        f.write(autre)
    refuser("gpt-etrangere", disque, "non vierge origine indeterminee")

    # Les BPB et les deux GPT restent valides mais le contenu de l'ESP diffère.
    disque = dossier / "esp-modifiee.raw"
    with cible.open("rb") as src, disque.open("xb") as dst:
        shutil.copyfileobj(src, dst)
    with disque.open("r+b") as f:
        f.seek(bilan["gpt"]["p9"] * secteur - 1)
        f.write(b"X")
    refuser("esp-modifiee", disque, "indetermine verification ESP")

    for etape in ("esp", "p9"):
        disque = nouvelle("coupure-" + etape)
        def couper(vm, canal, serie, trace):
            attendre(vm, serie, PROMPT.encode())
            canal.sendall(f"AUTORISER {SERIE} {taille}\r".encode())
            debut = bilan["gpt"][etape] * secteur // 512
            fin = (bilan["gpt"]["p9"] if etape == "esp" else bilan["gpt"]["fin_p9"]+1) * secteur // 512
            limite = time.monotonic()+180
            with trace.open("rb") as journal:
                while time.monotonic() < limite:
                    position = journal.tell()
                    ligne = journal.readline()
                    if not ligne.endswith(b"\n"):
                        journal.seek(position)
                        time.sleep(.005)
                        continue
                    m = re.match(rb"virtio_blk_handle_write .* sector (\d+) nsectors", ligne)
                    if m and debut <= int(m[1]) < fin:
                        qmp(vm, "stop")
                        assert b"INSTALLATION bilan Ok" not in serie.read_bytes()
                        return dict(premier_lba_512=int(m[1]), fin=compteurs(vm))
            raise RuntimeError("aucune écriture observée dans " + etape)
        essai("coupure-"+etape, disque, source, couper, coupure=True)
        refuser("refus-coupure-"+etape, disque, "metadonnees incoherentes",
                "non vierge origine indeterminee", "indetermine lecture P9", "indetermine verification ESP")



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
        assert empreinte(source) == bilan["support"]


if __name__=="__main__":
    main()
