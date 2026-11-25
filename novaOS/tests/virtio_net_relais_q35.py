"""Relais OVMF/Limine/NovaOS sous Secure Boot sur Q35, sans autorisation DMA du NIC."""
if not __debug__:
    raise RuntimeError("Les contrôles de qualification exigent les assertions Python.")

import argparse
from hashlib import blake2b
import json
import os
from pathlib import Path
import re
import shutil
import socket
import struct
import subprocess
import tempfile
import time
import resource

from contextlib import nullcontext
from virtio_net_pair import Pair
from ovmf_reseau_q35 import arguments, commandes_pci, pci_lecture
from virtio_net_confinement import ROOT, capture, digest, dump, qmp, reg
from virtio_net_qemu import qualification


def refus_liminer(serie):
    ligne = re.sub(rb"\x1b\[[0-?]*[ -/]*[@-~]", b"", serie)
    return b"PANIC: Blake2b hash for URI `boot():/novaos.elf` does not match!" in ligne


def support(elf, firmware, out, limine, retrait=False, temoin_hash=False, ethernet=False):
    from virt.firmware.varstore import autodetect
    from virt.peutils.pesign import pe_authenticode_hash
    import pefile
    esp = out / "esp"
    efi = esp / "EFI/BOOT/BOOTX64.EFI"
    efi.parent.mkdir(parents=True)
    shutil.copyfile(ROOT / "third_party/limine/BOOTX64.EFI", efi)
    shutil.copyfile(elf, esp / "novaos.elf")
    conf = esp / "limine.conf"
    conf.write_text("timeout: 0\nserial: yes\neditor_enabled: no\nhash_mismatch_panic: yes\n"
                    "\n/NovaOS\n    protocol: limine\n    path: boot():/novaos.elf#"
                    + blake2b(elf.read_bytes()).hexdigest() + "\n    cmdline: " + ("net-q35-ethernet" if ethernet else "net-q35-retrait" if retrait else "net-q35-relais") + "\n")
    subprocess.run([limine, "enroll-config", efi, blake2b(conf.read_bytes()).hexdigest()], check=True)
    h = pe_authenticode_hash(pefile.PE(str(efi))).hex()
    subprocess.run(["virt-fw-vars","-i",firmware/"OVMF_VARS.fd","-o",out/"variables.fd",
                    "--enroll-generate","NovaOS Q35 relais","--microsoft-kek","none","--no-microsoft",
                    "--add-db-hash","7a58904e-20f7-47ae-9f7c-674f87ef1104",h,"--sb"],check=True)
    v=autodetect.open_varstore(str(out/"variables.fd")).get_varlist()
    assert v["SecureBootEnable"].data==b"\x01" and v["PK"].data and bytes.fromhex(h) in v["db"].data
    if temoin_hash:
        # La copie isolée demeure un ELF chargeable. Une seule donnée ajoutée après scellement.
        with (esp/"novaos.elf").open("ab") as f:
            f.write(b"NovaOS qualification empreinte")
    subprocess.run(["qemu-img","convert","-f","raw","-O","raw",f"fat:{esp}",out/"support.raw"],check=True)
    dump(out/"entrees.json",{p.name:digest(p) for p in (esp/"novaos.elf",efi,conf,out/"support.raw",out/"variables.fd",firmware/"OVMF_CODE.fd")})


def verifier_tables(vm,qt,journal,out,retrait=False,cycles=1):
    m=re.search(rb"NET Q35 domaine=3 sid=56 iova=0x10000000 root=(0x[0-9a-f]+) qi=(0x[0-9a-f]+) recu=(0x[0-9a-f]+) cookie=2 blocs=22 cadres=30", journal)
    if not m:
        raise ValueError("Confirmation du noyau absente : "+repr(journal[-300:]))
    root,qi,recu=(int(v,16) for v in m.groups())
    def reg64(off):
        return reg(vm,off) | reg(vm,off+4)<<32
    base=0xfed90000
    assert reg64(base+0x20)==root and reg(vm,base+0x1c)==0xc4000000 and reg(vm,base+0x34)==0
    fin, cookie = (48+96*cycles, 1+2*cycles) if retrait else (96, 2)
    assert reg64(base+0x90)==qi and reg64(base+0x80)==reg64(base+0x88)==fin and reg(vm,recu)==cookie
    pages=set()
    def unique(pa):
        if pa%4096 or not 0<pa<=256*1024**2-4096 or pa in pages: raise ValueError("Cadre hors RAM ou partagé")
        pages.add(pa)
    def page(pa):
        unique(pa)
        qt.write(f"read {pa:#x} 4096\n".encode())
        line=qt.readline().strip()
        if not re.fullmatch(rb"OK 0x[0-9a-fA-F]{8192}",line): raise ValueError("Lecture qtest incomplète")
        words=struct.unpack("<512Q",bytes.fromhex(line[5:].decode()))
        return {i:v for i,v in enumerate(words) if v}
    def entree(entries,index,flags):
        if set(entries)!={index} or entries[index]&4095!=flags: raise ValueError("Portée VT-d inattendue")
        return entries[index]&~4095
    unique(qi); unique(recu)
    racine = page(root)
    if retrait:
        retraits=re.findall(rb"NET Q35 retrait cookie=(\d+) racine_vide=true rendus=27 permanents=3",journal)
        if racine or [int(v) for v in retraits]!=list(range(3,cookie+1,2)):
            raise ValueError("Racine ou compteurs incohérents après le retrait")
        dump(out/"tables.json", dict(racine=root, entrees=racine, iqa=qi, recu=recu, cookie=cookie,
                                      iqt=fin, iqh=fin, gsts=0xc4000000, cadres_rendus=27, cadres_permanents=3))
        return len(pages)
    ctx=entree(racine,0,1)
    c=page(ctx)
    if c.pop(113,None)!=0x302: raise ValueError("Domaine du NIC inattendu")
    pa=entree(c,112,1)
    for shift in (39,30,21):
        pa=entree(page(pa),(0x10000000>>shift)&511,3)
    leaf=page(pa)
    assert set(leaf)==set(range(22))
    for i,v in leaf.items():
        assert v&4095==(2 if i in (2,5,*range(6,14)) else 1)
        unique(v&~4095)
    assert len(pages)==30
    dump(out/"tables.json",dict(racine=root,contextes=ctx,feuille=leaf,iqa=qi,recu=recu,cadres_distincts=len(pages),gsts=0xc4000000))
    return len(pages)


def run(a,out):
    faute=os.environ.get("NOVAOS_NET_MODELE_FAUTE")=="injecter"
    if a.ethernet:
        from virtio_net_completion_absente import prerequis
        prerequis("Q35",a.elf)
    args=arguments(a.firmware/"OVMF_CODE.fd",out,out,False)
    args.remove("-no-reboot")
    if a.event_idx is not None:
        args.extend(["-global", "virtio-net-pci.event_idx=" + a.event_idx])
    if a.ethernet:
        for event in ("vtd_iotlb_page_update","vtd_iotlb_page_hit","vtd_dmar_fault","vtd_dmar_translate","vtd_inv_desc_iotlb_global"):
            args.extend(["-trace",event])
    args+=["-device","ramfb","-no-shutdown","-d","guest_errors,cpu_reset"]
    dump(out/"arguments.json",args)
    report={}
    try:
        with qualification(ROOT.parent.parent,report), tempfile.TemporaryDirectory(dir=os.environ["NOVAOS_TMPDIR"]) as temp:
            start=time.monotonic()
            usage=resource.getrusage(resource.RUSAGE_CHILDREN)
            with Pair(out) if a.ethernet else nullcontext(None) as pair:
                fds = pair.connecter(args) if pair else ()
                dump(out/"arguments.json",args)
                with (out/"qemu.trace").open("xb") as log, subprocess.Popen(args,cwd=temp,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=log,bufsize=0,pass_fds=fds) as vm:
                    try:
                        qmp(vm,"qmp_capabilities")
                        with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as sock:
                            sock.settimeout(5)
                            sock.connect(str(Path(temp)/"qtest.sock"))
                            with sock.makefile("rwb",buffering=0) as qt:
                                init=capture(vm,out,"reset",False)
                                assert init["iommu_platform"] is True and init["vtd_gsts"]==0 and pci_lecture(vm,qt)&4==0
                                qmp(vm,"cont")
                                end=time.monotonic()+(1800 if pair and pair.lots>=6250 else 35)
                                while time.monotonic()<end:
                                    if pair: pair.observer_pression(vm)
                                    serial=(out/"serie.log").read_bytes()
                                    if b"NET Q35 etat=" in serial or (a.temoin_hash and refus_liminer(serial)) or b"Panique du noyau" in serial or not qmp(vm,"query-status")["running"]: break
                                    time.sleep(0.05)
                                dump(out/"etat-avant-arret.json",qmp(vm,"query-status"))
                                qmp(vm,"stop")
                                fin=capture(vm,out,"fin",False)
                                assert pci_lecture(vm,qt)&4==0 and fin["pci_command"]&4==0
                                serial=(out/"serie.log").read_bytes()
                                report["bar4_bas"]=pci_lecture(vm,qt,0x20,4)
                                report["bar4_haut"]=pci_lecture(vm,qt,0x24,4)
                                report["ligne_et_pin"]=pci_lecture(vm,qt,0x3c,4)&65535
                                if a.temoin_hash:
                                    if not refus_liminer(serial) or b"NET Q35" in serial or fin["vtd_gsts"]!=0:
                                        raise ValueError("Refus explicite du noyau altéré absent : "+repr(serial[-480:]))
                                    report["noyau_altere_refuse"]=True
                                elif a.ethernet and faute:
                                    from virtio_net_completion_absente import constater
                                    report["cadres_quarantaine_controle_compris"]=verifier_tables(vm,qt,serial,out,retrait=False)
                                    constater(vm,out,"Q35")
                                elif a.ethernet:
                                    for motif in [b"NET L2 completions TX=16 RX=16 identifiants_uniques=true", b"NET Q35 L2 GSI=23 ISR=0x101", b"NET L2 pilote fin=Exit(0) racines=0", b"NET Q35 etat=Ok(()) bail=false"]:
                                        if motif not in serial:
                                            raise ValueError(f"Preuve invitée absente {motif!r}: {serial[-450:]!r}")
                                    total=pair.lots*16
                                    tx=32 if pair.performance=="rx" else total
                                    rx=32 if pair.performance=="tx" else total
                                    if f"NET L2 total TX={tx} RX={rx} index={rx%65536} rearmements={pair.lots-1}".encode() not in serial:
                                        raise ValueError("Indices terminaux ou réarmements incomplets")
                                    report["cadres_verifies"]=verifier_tables(vm,qt,serial,out,True,pair.cycles)
                                elif a.event_idx is not None:
                                    from virtio_net_event_idx import verifier_negociation
                                    report["negociation"] = verifier_negociation(out, a.event_idx, "Q35")
                                    status = b'Err("negociation EVENT_IDX")' if a.event_idx == "off" else b"Ok(())"
                                    if b"NET Q35 etat=" + status + b" bail=false" not in serial:
                                        raise ValueError("Statut terminal EVENT_IDX absent")
                                    report["cadres_verifies"] = verifier_tables(vm, qt, serial, out, retrait=True)
                                elif b"NET Q35 etat=Ok(()) bail=false" not in serial:
                                    raise ValueError("Verdict invité absent : "+repr(serial[-600:]))
                                else:
                                    report["cadres_verifies"]=verifier_tables(vm,qt,serial,out,a.retrait)
                        qmp(vm,"quit")
                        if vm.wait(timeout=5): raise RuntimeError("Sortie QEMU anormale")
                    finally:
                        if vm.poll() is None: vm.kill()
                        vm.wait()
                        report["execution_s"]=time.monotonic()-start
                        fin_cpu=resource.getrusage(resource.RUSAGE_CHILDREN)
                        dump(out/"cpu-qemu.json",dict(reel_s=report["execution_s"],
                             utilisateur_s=fin_cpu.ru_utime-usage.ru_utime,systeme_s=fin_cpu.ru_stime-usage.ru_stime))
            with (out/"qemu.trace").open() as journal:
                writes=commandes_pci(journal)
            if a.ethernet:
                import json
                if not json.loads((out/"pair.json").read_text())["succes"] or not any(w["valeur"]&4 for w in writes):
                    raise ValueError("Preuve bidirectionnelle du pair ou BME absente")
                import bisect
                attributions=[]
                bme=False
                for w in writes:
                    actif=bool(w["valeur"] & 4)
                    if actif and not bme:
                        attributions.append(w["ligne"])
                    bme=actif
                if len(attributions)!=pair.cycles or bme:
                    raise ValueError("Nombre de fenêtres Bus Master incorrect")
                pages=[{} for _ in attributions]
                feuille=re.compile(r"vtd_iotlb_page_(?:update|hit) .*?sid 0x38 iova (0x[0-9a-f]+) slpte (0x[0-9a-f]+) domain 0x3$")
                with (out/"qemu.trace").open() as journal:
                    for numero,ligne in enumerate(journal,1):
                        if ligne.startswith("vtd_dmar_fault "):
                            raise ValueError("Faute VT-d")
                        if not ligne.startswith("vtd_iotlb_page_") or "sid 0x38 " not in ligne:
                            continue
                        m=feuille.fullmatch(ligne.rstrip())
                        epoch=bisect.bisect_right(attributions,numero)-1
                        if not m or epoch<0:
                            raise ValueError("Traduction hors du domaine ou du bail")
                        adresse,valeur=(int(v,16) for v in m.groups())
                        i=(adresse-0x10000000)//4096
                        if not 0<=i<22 or valeur&3!=(2 if i in (2,5,*range(6,14)) else 1) or pages[epoch].setdefault(i,valeur)!=valeur:
                            raise ValueError("Traduction VT-d hors du domaine attendu")
                if any(len(p)!=22 or len({v>>12 for v in p.values()})!=22 for p in pages):
                    raise ValueError("Feuilles actives incomplètes ou partagées")
                report["pages_actives_3"]=22
                report["attributions_verifiees"]=len(pages)
                if pair.cycles>1:
                    from virtio_net_pair import verifier_redemarrages
                    verifier_redemarrages(serial.decode(),json.loads((out/"pair.json").read_text()))
                if faute:
                    report.update(trames_tx=16, trames_copiees_rx=16, completions_rx=15, quarantaine=True)
                else:
                    report.update(trames_tx=tx*pair.cycles,trames_rx=rx*pair.cycles)
                    if tx==rx: report["trames_par_sens"]=tx*pair.cycles
            else:
                assert not any(w["valeur"]&4 for w in writes)
            report.update(bilan=True,bail=False,retrait=a.retrait,ecritures_commande=writes)
    finally:
        dump(out/"campagne.json",report)


if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    for key in ("elf","firmware","limine","sortie"):
        p.add_argument(key,type=lambda s:Path(s).resolve())
    p.add_argument("--ethernet",action="store_true")
    p.add_argument("--retrait",action="store_true")
    p.add_argument("--temoin-hash",action="store_true")
    p.add_argument("--event-idx", choices=("on", "off"))
    a=p.parse_args()
    if a.ethernet and (a.event_idx!="on" or not os.environ.get("NOVAOS_QEMU_RX_Q35")):
        p.error("Le trafic exige un modèle privé corrigé et EVENT_IDX")
    if a.event_idx and (not a.retrait or a.temoin_hash):
        p.error("--event-idx exige --retrait et un noyau authentifié")
    a.sortie.mkdir()
    support(a.elf,a.firmware,a.sortie,a.limine,a.retrait,a.temoin_hash,a.ethernet)
    run(a,a.sortie)
