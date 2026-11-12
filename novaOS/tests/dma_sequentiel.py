"""Qualification du pflash privé et du transfert à la cible, sans installation."""
import argparse
from contextlib import nullcontext
from dataclasses import dataclass
import json
from pathlib import Path
import re
import socket
import subprocess
import tempfile
import time

from bloc import qmp
from installation_utm import FIRMWARE, UTM, empreinte, lanceurs
from profils_utm import disque_sequentiel
from prototype_uefi import image, pflash, tpm

MIO = 1 << 20
SERIE = "NOVA-CIBLE-000000001"
PROMPT = "CIBLE confirmation requise"
FERME = "CIBLE bilan Ok(())"
SHELL = "No mapping found"
MAP_OUT = "virtio_iommu_map domain=1 virt_start=0x8006000 "
DROITS_CIBLE = (1, 1, 3, 1, 2, 2, 1)


@dataclass(frozen=True)
class Cas:
    attendu: str
    tpm: str | None = "crb"
    taille: int = 64 * MIO
    accepter: bool = False
    signature: bytes = b""
    offset: int = 0
    source: int = 6
    cible: int = 7
    secteur: int = 512
    role_source: str = "NovaOS-Install-v1"
    role_cible: str = "NovaOS-Target-v1"
    serie: str = SERIE
    suspendu: bool = False
    source_ro: bool = True


CAS = {
    "support": Cas("CIBLE absente", taille=0),
    "vierge": Cas(PROMPT, accepter=True),
    "refus": Cas(PROMPT),
    "confirmation-erronee": Cas(PROMPT),
    "tis": Cas(PROMPT, tpm="tis", accepter=True),
    "deplaces": Cas(PROMPT, source=7, cible=6, secteur=4096),
    "reemploi": Cas(PROMPT),
    "redemarrage": Cas(PROMPT, accepter=True),
    "fin": Cas("CIBLE non vierge octet=67108863", signature=b"\1", offset=-1),
    "mbr": Cas("CIBLE non vierge octet=510", signature=b"\x55\xaa", offset=510),
    "gpt": Cas("CIBLE non vierge octet=512", signature=b"EFI PART", offset=512),
    "fat": Cas("CIBLE non vierge octet=82", signature=b"FAT32   ", offset=82),
    "novafs": Cas("CIBLE non vierge octet=4096", signature=b"NOVAP0FS", offset=4096),
    "trop-petit": Cas("CIBLE bilan Err(Geometry)", taille=4 * MIO),
    "erreur-lecture": Cas("BLOC erreur peripherique"),
    "mauvais-support": Cas(SHELL, role_source="NovaOS-Inconnu"),
    "doublon": Cas(SHELL, role_cible="NovaOS-Install-v1"),
    "roles-inverses": Cas(SHELL, role_source="NovaOS-Target-v1", role_cible="NovaOS-Install-v1"),
    "support-inscriptible": Cas(SHELL, source_ro=False),
    "tpm-absent": Cas("NOVA DXE QUARANTAINE TPM2 GetCapability", tpm=None),
    "mauvaise-serie": Cas("CIBLE bilan Err(Device)", serie="NOVA-AUTRE-000000001"),
    "quarantaine": Cas("NOVA DXE QUARANTAINE", suspendu=True),
    "coupure": Cas("NOVA DXE retrait anneau suspendu", suspendu=True),
}


def attendre(vm, fichier, texte, debut=0):
    limite = time.monotonic() + 120
    while time.monotonic() < limite and vm.poll() is None:
        serie = fichier.read_bytes()[debut:] if fichier.exists() else b""
        if texte.encode() in serie:
            return
        # Le PL011 peut avoir écrit « CIBLE bilan » sans avoir encore émis le résultat.
        lignes, _, _ = serie.rpartition(b"\n")
        if any(x in lignes for x in (b"Panique du noyau", b"Synchronous Exception", b"CIBLE bilan ",
                                     b"NOVA DXE QUARANTAINE", b"No mapping found")):
            break
        time.sleep(0.01)
    raise RuntimeError(f"{texte!r} absent, voir {fichier}")


def compteurs(vm):
    champs = ("rd_operations", "rd_bytes", "wr_operations", "wr_bytes", "flush_operations")
    return {d["device"]: {k: d["stats"][k] for k in champs} for d in qmp(vm, "query-blockstats")
            if d["device"] in ("source", "cible")}


def etat_pci(vm, slot):
    adresse = 0x4010000000 + slot * 8 * 4096 + 4
    ligne = qmp(vm, "human-monitor-command", **{"command-line": f"xp /1hx {adresse:#x}"})
    return int(ligne.split(":")[1].strip(), 16) & 4 != 0


def descripteurs_crb(vm):
    # Six mots de 32 bits avant la première instruction de la VM.
    reponse = qmp(vm, "human-monitor-command", **{"command-line": "xp /6wx 0xc000058"})
    valeurs = [int(mot, 16) for ligne in reponse.splitlines()
               for mot in ligne.partition(":")[2].split()]
    assert valeurs == [0xf80, 0xc000080, 0, 0xf80, 0xc000080, 0], reponse
    return valeurs


def observer(vm, canal, serie, trace, cas, nom):
    attendre(vm, serie, cas.attendu)
    avant = compteurs(vm)
    resultat = {"premier_jalon": cas.attendu, "au_jalon": avant}
    if cas.attendu == PROMPT:
        assert avant["cible"]["rd_bytes"] == cas.taille
        assert all(c["wr_operations"] == c["wr_bytes"] == 0 for c in avant.values())
        actif = trace.read_text().splitlines()
        assert any(x.startswith("virtio_iommu_map domain=1 virt_start=0x8005000 ") for x in actif)
        assert not any(x.startswith(MAP_OUT) for x in actif)
        resultat["page_out_au_jalon"] = False
        resultat["bme_source_a_la_confirmation"] = etat_pci(vm, cas.source)
        assert not resultat["bme_source_a_la_confirmation"]
        accepte = cas.accepter
        ligne = f"AUTORISER {SERIE} {cas.taille}" if accepte else "NON"
        if nom == "confirmation-erronee":
            ligne = f"AUTORISER {SERIE} {cas.taille - 512}"
        canal.sendall((ligne + "\r").encode())
        attendre(vm, serie, FERME)
        contenu = serie.read_bytes()
        assert (b"CIBLE droit WRITE accorde" in contenu) == accepte
        assert (b"CIBLE confirmation refusee" in contenu) != accepte
        assert not etat_pci(vm, cas.cible)
        resultat["confirmation"] = accepte
    elif cas.attendu.startswith(("CIBLE non vierge", "BLOC erreur", "CIBLE absente")):
        attendre(vm, serie, "CIBLE bilan Err(")
    fin = compteurs(vm)
    for stats in fin.values():
        # QEMU peut synchroniser son backend lors d'un reset de transport.
        assert stats["wr_operations"] == stats["wr_bytes"] == 0, fin
    if cas.attendu == SHELL or cas.tpm is None:
        assert all(c["rd_bytes"] == 0 for c in fin.values())
        assert not etat_pci(vm, cas.source) and not etat_pci(vm, cas.cible)
    if cas.taille and (cas.suspendu or cas.attendu == SHELL or nom in ("trop-petit", "mauvaise-serie")):
        assert fin["cible"]["rd_bytes"] == 0
    if cas.attendu == PROMPT:
        assert fin["cible"]["rd_bytes"] == (1 + int(accepte)) * cas.taille
    elif cas.attendu.startswith("CIBLE non vierge"):
        offset = cas.offset if cas.offset >= 0 else cas.taille + cas.offset
        assert fin["cible"]["rd_bytes"] == (offset // 4096 + 1) * 4096
    if cas.suspendu:
        assert not etat_pci(vm, cas.source) and not etat_pci(vm, cas.cible)
        assert b"NovaOS v0.14.0" not in serie.read_bytes()
    resultat["fin"] = fin
    return resultat


def demarrer(tmp, sortie, nom, cas, code, var, source, cible):
    serie = sortie / (nom + ".serial")
    trace = sortie / (nom + ".trace")
    console = tmp / "uart"
    args = [tmp / "qemu-aarch64-softmmu", "-L", FIRMWARE, "-machine", "virt-10.0,gic-version=3,its=off",
            "-accel", "tcg", "-cpu", "cortex-a72", "-m", "256", "-smp", "1", "-nic", "none",
            "-display", "none", "-device", "ramfb", "-monitor", "none", "-qmp", "stdio", "-S",
            "-chardev", f"socket,id=uart,path={console},server=on,wait=off,logfile={serie}",
            "-serial", "chardev:uart",
            "-drive", f"if=pflash,unit=0,format=raw,readonly=on,file={code}",
            "-drive", f"if=pflash,unit=1,format=raw,file={var}",
            "-device", "virtio-iommu-pci,addr=5,granule=4k,aw-bits=48,boot-bypass=off",
            "-drive", f"if=none,id=source,format=raw,readonly={'on' if cas.source_ro else 'off'},file={source}"]
    args += disque_sequentiel("source", cas.source, True, designation=cas.role_source)
    if cas.taille:
        backend = cible
        if nom == "erreur-lecture":
            config = tmp / "lecture.conf"
            config.write_text('[inject-error]\nevent = "read_aio"\nerrno = "5"\nsector = "16"\n')
            backend = f"blkdebug:{config}:{cible}"
        args += ["-drive", f"if=none,id=cible,format=raw,file={backend},rerror=report,werror=report"]
        dev = disque_sequentiel("cible", cas.cible, False, designation=cas.role_cible, serie=cas.serie)
        dev[1] += f",logical_block_size={cas.secteur},physical_block_size={cas.secteur}"
        args += dev
    for event in ("virtio_iommu_attach", "virtio_iommu_detach", "virtio_iommu_map",
                  "virtio_iommu_unmap_done", "virtio_iommu_report_fault", "virtio_blk_handle_write",
                  "virtio_blk_handle_read",
                  "gicv3_icc_eoir_write", "tpm_crb_mmio_write"):
        args += ["-trace", event]
    etat = tmp / "tpm-coupure" if nom in ("coupure", "reprise") else None
    if nom == "coupure":
        etat.mkdir()
    backend_tpm = tpm(tmp, sortie, nom, etat) if cas.tpm else nullcontext()
    with backend_tpm as sock, trace.open("xb") as log:
        if sock:
            args += ["-chardev", f"socket,id=tch,path={sock}", "-tpmdev", "emulator,id=tpm0,chardev=tch",
                     "-device", f"tpm-{cas.tpm}-device,tpmdev=tpm0"]
        with subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=log, bufsize=0) as vm:
            try:
                qmp(vm, "qmp_capabilities")
                assert not etat_pci(vm, cas.source)
                if cas.taille:
                    assert not etat_pci(vm, cas.cible)
                registres = descripteurs_crb(vm) if cas.tpm == "crb" else None
                with socket.socket(socket.AF_UNIX) as canal:
                    canal.connect(str(console))
                    qmp(vm, "cont")
                    phases = [observer(vm, canal, serie, trace, cas, nom)]
                    if registres:
                        phases[0]["crb_mots_0x58_a_0x6c_avant_uefi"] = registres
                    if nom == "redemarrage":
                        pos = serie.stat().st_size
                        position_trace = trace.stat().st_size
                        qmp(vm, "system_reset")
                        # QMP conserve les compteurs backend au reset. Nouveau zéro lu complet vérifié par différence.
                        compteurs_avant = phases[0]["fin"]
                        attendre(vm, serie, PROMPT, pos)
                        stats = compteurs(vm)
                        assert stats["cible"]["rd_bytes"] - compteurs_avant["cible"]["rd_bytes"] == cas.taille
                        with trace.open("rb") as journal:
                            journal.seek(position_trace)
                            assert MAP_OUT.encode() not in journal.read()
                        canal.sendall(b"NON\r")
                        attendre(vm, serie, FERME, pos)
                        assert b"droit WRITE accorde" not in serie.read_bytes()[pos:]
                        assert compteurs(vm)["cible"]["wr_bytes"] == 0
                        phases.append({"confirmation": False, "page_out_au_jalon": False, "fin": compteurs(vm)})
            finally:
                vm.kill()
                vm.wait()
                console.unlink(missing_ok=True)
    return phases


def traces(sortie, nom, cas):
    texte = (sortie / (nom + ".trace")).read_text()
    console = (sortie / (nom + ".serial")).read_text(errors="replace")
    attaches = re.findall(r"^virtio_iommu_attach domain=(\d+) endpoint=(\d+)$", texte, re.M)
    conforme = [] if cas.attendu == SHELL or cas.tpm is None else [("2", str(cas.source * 8))]
    if conforme and cas.taille and not cas.suspendu:
        conforme.append(("1", str(cas.cible * 8)))
    assert attaches == conforme * (2 if nom == "redemarrage" else 1), (nom, attaches)
    assert "virtio_blk_handle_write " not in texte and "virtio_iommu_report_fault " not in texte
    if attaches:
        assert "NOVA TPM GetCapability fabricant=" in console
    droits_out = sum(x.startswith(MAP_OUT) for x in texte.splitlines())
    assert droits_out == int(cas.accepter)
    # START atteste du transport CRB. Aucune écriture ne doit reprogrammer ses descripteurs.
    crb_start = len(re.findall(r"^tpm_crb_mmio_write CRB write 0x0*4c len:4 val: 0x1$", texte, re.M))
    assert bool(crb_start) == (cas.tpm == "crb")
    assert not re.search(r"^tpm_crb_mmio_write CRB write 0x0*(58|5c|60|64|68|6c) ", texte, re.M)
    actifs = set()
    domaine = None
    pages = set()
    lecteurs = {}
    lectures = {1: 0, 2: 0}
    irq = 35 + cas.cible % 4
    eoi = 0
    for ligne in texte.splitlines():
        if ligne.startswith("virtio_iommu_attach "):
            assert domaine is None and not actifs
            domaine = int(re.search(r"domain=(\d+)", ligne)[1])
        elif ligne.startswith("virtio_iommu_detach "):
            assert not actifs and f"domain={domaine} " in ligne
            domaine = None
        elif ligne.startswith(("virtio_iommu_map ", "virtio_iommu_unmap_done ")):
            d, lo, hi = re.search(r"domain=(\d+) virt_start=(0x\w+) virt_end=(0x\w+)", ligne).groups()
            assert int(d) == domaine
            plage = (lo, hi)
            if ligne.startswith("virtio_iommu_map "):
                assert plage not in actifs
                actifs.add(plage)
                debut, fin = int(lo, 16), int(hi, 16)
                if domaine == 2:
                    pa = int(re.search(r"phys_start=(0x\w+)", ligne)[1], 16)
                    pages.update(range(pa, pa + fin - debut + 1, 4096))
                else:
                    page, offset = divmod(debut - 0x08000000, 4096)
                    assert 0 <= page < len(DROITS_CIBLE) and offset == 0 and fin == debut + 4095
                    assert int(re.search(r"flags=(\d+)", ligne)[1]) == DROITS_CIBLE[page]
            else:
                actifs.remove(plage)
        elif domaine == 1 and ligne.startswith("gicv3_icc_eoir_write ") and ligne.endswith(f" value {irq:#x}"):
            eoi += 1
        elif ligne.startswith("virtio_blk_handle_read "):
            assert domaine in lectures and actifs
            objet = re.search(r"vdev (0x[0-9a-fA-F]+) ", ligne)[1]
            assert lecteurs.setdefault(domaine, objet) == objet
            assert lecteurs.get(3 - domaine) != objet
            lectures[domaine] += 1
    if cas.suspendu:
        assert domaine == 2 and len(actifs) == 2
        assert "NOVA DXE SORTIE" not in console
    else:
        assert domaine is None and not actifs
    if ("1", str(cas.cible * 8)) in attaches:
        assert eoi > 0
        assert "NOVA DXE transfert cible BME=0 domaine=aucun" in console
    dispos = set()
    for lo, hi in re.findall(r"DMA pages reutilisees (0x\w+) (0x\w+)", console):
        dispos.update(range(int(lo, 16), int(hi, 16), 4096))
    if nom == "reemploi":
        assert pages and pages <= dispos
    return dict(attaches=attaches, plages_actives_finales=len(actifs), maps_page_out=droits_out,
                lectures_bloc_uefi=lectures[2], lectures_bloc_novaos=lectures[1],
                commandes_crb_start=crb_start, intid_cible=irq if eoi else None, eoi_cible=eoi,
                pages_firmware=len(pages), pages_reallouees=len(dispos & pages))


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("fv", type=Path)
    p.add_argument("noyau", type=Path)
    p.add_argument("sortie", type=Path, help="nouveau répertoire de preuves")
    p.add_argument("--fv-suspendu", type=Path)
    p.add_argument("--cas", nargs="+", choices=CAS, default=list(CAS))
    a = p.parse_args()
    if any(CAS[n].suspendu for n in a.cas) and not a.fv_suspendu:
        p.error("--fv-suspendu requis pour les cas négatifs du transfert")
    a.sortie.mkdir()
    with tempfile.TemporaryDirectory(prefix="nvseq-", dir="/tmp") as lab:
        tmp = Path(lab)
        lanceurs(tmp)
        version = subprocess.check_output([tmp / "qemu-aarch64-softmmu", "--version"], text=True).splitlines()[0]
        utm = subprocess.check_output(
            ["plutil", "-extract", "CFBundleShortVersionString", "raw", UTM / "Info.plist"], text=True
        ).strip()
        assert (version, utm) == ("QEMU emulator version 10.0.12", "5.0.8")
        code = tmp / "code.fd"
        pflash(a.fv / "QEMU_EFI.fd", code)
        negatif = tmp / "suspendu.fd"
        if a.fv_suspendu:
            pflash(a.fv_suspendu / "QEMU_EFI.fd", negatif)
            assert empreinte(code) != empreinte(negatif)
        source = image(tmp / "source", a.noyau, "dma-sequentiel")
        reemploi = image(tmp / "reemploi", a.noyau, "dma-sequentiel-reemploi") if "reemploi" in a.cas else None
        bilan = dict(moteur=version, utm=utm, pflash_sha256=empreinte(code), noyau_sha256=empreinte(a.noyau),
                     source_sha256=empreinte(source), essais={})
        for nom in a.cas:
            cas = CAS[nom]
            cible = tmp / (nom + ".raw")
            var = tmp / (nom + ".vars")
            pflash(a.fv / "QEMU_VARS.fd", var)
            if cas.taille:
                with cible.open("xb") as f:
                    f.truncate(cas.taille)
                    f.seek(cas.offset, 2 if cas.offset < 0 else 0)
                    f.write(cas.signature)
                avant = empreinte(cible)
            duree = time.monotonic()
            phases = demarrer(tmp, a.sortie, nom, cas, negatif if cas.suspendu else code,
                              var, reemploi if nom == "reemploi" else source, cible)
            res = dict(phases=phases, secondes=round(time.monotonic() - duree, 3), **traces(a.sortie, nom, cas))
            if nom == "coupure":
                normal = Cas(PROMPT)
                res["reprise"] = demarrer(tmp, a.sortie, "reprise", normal, code, var, source, cible)
                res["traces_reprise"] = traces(a.sortie, "reprise", normal)
            if cas.taille:
                assert empreinte(cible) == avant
                res["cible_sha256_avant_apres"] = avant
            bilan["essais"][nom] = res
            print(nom, res["secondes"], res["attaches"], flush=True)
            (a.sortie / "resultats.json").write_text(json.dumps(bilan, indent=2, ensure_ascii=False) + "\n")
        assert empreinte(source) == bilan["source_sha256"]


if __name__ == "__main__":
    main()
