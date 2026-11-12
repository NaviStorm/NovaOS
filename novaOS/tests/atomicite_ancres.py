"""Coupures de publication P9 sur le moteur AArch64 distribué dans UTM 5.0.8."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import csv
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import socket
import struct
import subprocess
import tarfile
import tempfile
import uuid

from bloc import qmp
from dma_sequentiel import PROMPT, SERIE, compteurs
from installation_gpt import attendre, controle_domaines, machine, verifier_gpt
from installation_utm import FIRMWARE, ROOT, UTM, empreinte, lanceurs
from profils_utm import disque_sequentiel
from prototype_uefi import image, pflash, tpm

MIO = 1 << 20
FLUSH = 0xffffffff
ARCHIVE = ROOT.parent / "distribution/images/novaos-0.15.0-utm-aarch64.tar.gz"
SOMME = "1f7cc3404418a80e4f5c11e4af162e9d93754e7d8c829bf0d7b1c4be6af833cb"
TEST = "fs::remplacement::utm::images_du_banc_utm"


def rust(corpus, arch, journal):
    env = dict(os.environ, NOVAFS_TARGET=f"{arch}-apple-darwin", NOVAFS_ANCRES_UTM=str(corpus))
    with journal.open("xb") as f:
        subprocess.run([ROOT.parent / "novaFS/lab/run", "test", "--offline", "--lib", TEST,
                        "--", "--ignored", "--exact", "--nocapture"], stdout=f, stderr=f, env=env, check=True)
    assert b"1 passed; 0 failed" in journal.read_bytes()


@dataclass(frozen=True)
class Cas:
    secteur: int
    fmt: str
    operation: str
    phase: str
    decalage: int = 0
    plage: tuple | None = None

    @property
    def nom(self):
        return f"{self.secteur}-{self.fmt}-{self.operation}-{self.phase}-{self.decalage}" + (
            f"-{self.plage[0]}-{self.plage[1]}" if self.plage else "")


def matrice():
    for s in (512, 4096):
        for fmt in ("raw", "qcow2"):
            for op in ("mv", "cp"):
                for phase in ("avant", "preparation", "avant-F1", "pendant-F1", "apres-F1",
                              "pendant-ancre", "premier-secteur", "avant-F2", "pendant-F2",
                              "apres-F2", "succes", "arret-ordonne"):
                    yield Cas(s, fmt, op, phase)
            # Une coupure entre commandes entières laisse exactement ce préfixe
            # dans une ancre décalée. La GPT livrée n'utilise pas ce placement.
            for i, prefixe in enumerate((1, 32, 64, 96, 128, 184, 256)):
                yield Cas(s, fmt, ("mv", "cp")[i % 2], "premier-secteur", s-prefixe)
            if s == 512:
                yield Cas(s, fmt, "mv", "premier-secteur", 512)
            # Contrôles négatifs volontaires : modification partielle du contenu
            # par lecture-modification-écriture d'un ou plusieurs secteurs entiers.
            for plage in ((0, 96), (64, 32), (136, 48), (511, 2)):
                yield Cas(s, fmt, "mv", "avant-F2", plage=plage)


class Plan:
    def __init__(self, cas, corpus):
        self.cas = cas
        s = cas.secteur
        self.base = (61 if s == 512 else 265)*MIO
        self.capacite = (64 if s == 512 else 268)*MIO
        self.dossier = corpus / f"{s}-{cas.operation}"
        self.avant = (self.dossier / "avant.raw").read_bytes()
        self.apres = (self.dossier / "apres.raw").read_bytes()
        assert len(self.avant) == len(self.apres) == 2*MIO
        ordre = (self.dossier / "ordre.txt").read_text().splitlines()
        self.slot = int(ordre[-2].split()[1])
        assert self.slot in (0, 1) and ordre[-3:] == ["F1", f"W {self.slot}", "F2"]
        self.origine = self.base-s
        self.debut = s+cas.decalage
        fenetre = bytearray([0xa5])*(len(self.avant)+3*s)
        fenetre[self.debut:self.debut+len(self.avant)] = self.avant
        self.initial = bytes(fenetre)
        self.ops = []
        for ligne in ordre:
            if ligne in ("F1", "F2"):
                self.ops.append((ligne, FLUSH, b""))
                continue
            bloc = int(ligne.split()[1])
            etiquette = "ancre" if bloc == self.slot else "préparation"
            off, n = cas.plage if cas.plage and etiquette == "ancre" else (0, 4096)
            source = 4096*bloc+off
            i = self.debut+source
            fenetre[i:i+n] = self.apres[source:source+n]
            for pos in range(i//s*s, i+n, s):
                self.ops.append((etiquette, (self.origine+pos)//s, bytes(fenetre[pos:pos+s])))
        noms = [label for label, _, _ in self.ops]
        f1, ancre, f2 = (noms.index(label) for label in ("F1", "ancre", "F2"))
        self.pause = {"avant": 0, "preparation": 1, "avant-F1": f1, "pendant-F1": f1,
                      "apres-F1": f1+1, "pendant-ancre": ancre, "premier-secteur": ancre+1,
                      "avant-F2": f2, "pendant-F2": f2, "apres-F2": f2+1,
                      "succes": f2+1, "arret-ordonne": f2+1}[cas.phase]
        self.arret = {"pendant-F1": "flush_to_disk", "pendant-ancre": "write_aio",
                      "pendant-F2": "flush_to_disk"}.get(cas.phase)
        self.marque = b"NVP9LAB1" + struct.pack("<I", s) + uuid.uuid4().bytes

    def preparer(self, dossier):
        brut = dossier / "vierge-marque.raw"
        with brut.open("xb") as f:
            f.truncate(self.capacite)
            f.write(self.marque)
            f.seek(self.origine)
            f.write(self.initial)
            f.flush()
            os.fsync(f.fileno())
        cible = dossier / f"cible.{self.cas.fmt}"
        convertir(brut, "raw", cible, self.cas.fmt)
        arguments = dossier / "requetes.bin"
        with arguments.open("xb") as f:
            f.write(self.marque + struct.pack("<I", self.pause))
            for _, lba, data in self.ops:
                f.write(struct.pack("<I", lba) + data)
        assert arguments.stat().st_size <= 65536
        return cible, arguments

    def relire(self, disque):
        s = self.cas.secteur
        attendu = bytearray(self.initial)
        for _, lba, page in self.ops[:self.pause]:
            if lba != FLUSH:
                index = lba*s-self.origine
                attendu[index:index+s] = page
        with disque.open("rb") as f:
            assert f.read(s) == self.marque.ljust(s, b"\0")
            verifier_zeros(f, self.origine-s)
            fenetre = f.read(len(attendu))
            verifier_zeros(f, self.capacite-self.origine-len(attendu))
            assert f.read(1) == b""
        assert fenetre == attendu, f"octets différents des commandes complètes à {self.cas.nom}"
        volume = fenetre[self.debut:self.debut+len(self.avant)]
        ancre = slice(4096*self.slot, 4096*(self.slot+1))
        etat = ("ancien" if volume[ancre] == self.avant[ancre] else
                "nouveau" if volume[ancre] == self.apres[ancre] else "refus")
        if self.cas.phase in ("succes", "apres-F2", "arret-ordonne"):
            assert volume == self.apres and etat == "nouveau"
        digest = hashlib.sha256(volume).hexdigest()
        # Le préfixe de la géométrie et le cas rendent le nom exclusif à cette VM.
        fichier = f"{self.cas.nom}.raw"
        with (self.dossier / fichier).open("xb") as f:
            f.write(volume)
        return etat, digest, fichier


def verifier_zeros(f, n):
    vide = bytes(MIO)
    while n:
        taille = min(n, len(vide))
        assert f.read(taille) == vide[:taille]
        n -= taille


def convertir(source, fmt, cible, sortie):
    subprocess.run(["qemu-img", "convert", "-f", fmt, "-O", sortie, source, cible], check=True)


def moniteur(vm, commande):
    retour = qmp(vm, "human-monitor-command", **{"command-line": f'qemu-io cible "{commande}"'})
    if retour:
        raise RuntimeError(retour)


def virtualiser(lanceur, commun, travail, sortie, plan, cible, arguments):
    cas = plan.cas
    s = cas.secteur
    serie = sortie / f"{cas.nom}.serial"
    trace = sortie / f"{cas.nom}.trace"
    var = travail / "vars.fd"
    shutil.copyfile(commun / "vars.fd", var)
    io = (f"file.driver=blkdebug,file.image.driver=file,file.image.filename={cible}" if plan.arret
          else f"file={cible}")
    disque = disque_sequentiel("cible", 7, False)
    disque[1] += f",logical_block_size={s},physical_block_size={s}"
    with tempfile.TemporaryDirectory(prefix="p9-", dir="/tmp") as court:
        court = Path(court)
        uart = court / "uart"
        with tpm(lanceur, sortie, cas.nom) as sock, trace.open("xb") as log:
            cmd = [lanceur / "qemu-aarch64-softmmu", "-L", FIRMWARE,
                   "-machine", "virt-10.0,gic-version=3,its=off,virtualization=off", "-accel", "tcg",
                   "-cpu", "cortex-a72", "-smp", "1", "-m", "256", "-nic", "none", "-device", "ramfb",
                   "-display", "none", "-monitor", "none", "-qmp", "stdio", "-S",
                   "-chardev", f"socket,id=uart,path={uart},server=on,wait=off,logfile={serie}",
                   "-serial", "chardev:uart", "-chardev", f"socket,id=tch,path={sock}",
                   "-tpmdev", "emulator,id=tpm0,chardev=tch", "-device", "tpm-crb-device,tpmdev=tpm0",
                   "-drive", f"if=pflash,unit=0,format=raw,readonly=on,file={commun/'code.fd'}",
                   "-drive", f"if=pflash,unit=1,format=raw,file={var}",
                   "-device", "virtio-iommu-pci,addr=5,granule=4k,aw-bits=48,boot-bypass=off",
                   "-fw_cfg", f"name=opt/novaos/ancres-p9-lab,file={arguments}",
                   "-drive", f"if=none,id=source,format=raw,readonly=on,file={commun/'support/disque.raw'}",
                   *disque_sequentiel("source", 6, True),
                   "-drive", f"if=none,id=cible,format={cas.fmt},{io},discard=unmap,detect-zeroes=unmap,"
                             "rerror=report,werror=report", *disque]
            for event in ("virtio_blk_handle_write", "virtio_iommu_attach", "virtio_iommu_detach",
                          "virtio_iommu_report_fault"):
                cmd += ["-trace", event]
            with subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                  stderr=log, bufsize=0) as vm:
                try:
                    qmp(vm, "qmp_capabilities")
                    cible_qmp = next(d["inserted"] for d in qmp(vm, "query-block") if d["device"]=="cible")
                    cache = cible_qmp["cache"]
                    assert cible_qmp["drv"] == cas.fmt and cache == {"writeback": True, "direct": False, "no-flush": False}
                    geometrie = [qmp(vm, "qom-get", path="/machine/peripheral/destination", property=p)
                                 for p in ("logical_block_size", "physical_block_size")]
                    assert geometrie == [s, s]
                    with socket.socket(socket.AF_UNIX) as canal:
                        canal.connect(str(uart))
                        qmp(vm, "cont")
                        attendre(vm, serie, f"P9LAB pret secteur={s}\r\n".encode(), timeout=65)
                        initial = compteurs(vm)
                        assert initial["cible"]["wr_operations"] == 0
                        canal.sendall(b"S")
                        attendre(vm, serie, f"P9LAB pause {plan.pause}/{len(plan.ops)}\r\n".encode())
                        if plan.arret:
                            moniteur(vm, f"break {plan.arret} coupure")
                            canal.sendall(b"G")
                            moniteur(vm, "wait_break coupure")
                        elif cas.phase in ("succes", "arret-ordonne"):
                            canal.sendall(b"G")
                            attendre(vm, serie, b"P9LAB succes apres F2\r\n")
                        fin = compteurs(vm)
                        assert fin["source"]["wr_operations"] == 0
                        ecrits = sum(lba != FLUSH for _, lba, _ in plan.ops[:plan.pause])
                        synchronises = plan.pause-ecrits
                        assert fin["cible"]["wr_operations"] == ecrits
                        assert fin["cible"]["wr_bytes"] == ecrits*s
                        assert fin["cible"]["flush_operations"]-initial["cible"]["flush_operations"] == synchronises
                        if cas.phase == "arret-ordonne":
                            canal.sendall(b"C")
                            attendre(vm, serie, b"P9LAB fin Ok(())")
                            qmp(vm, "quit")
                        else:
                            vm.kill()
                    assert vm.wait(timeout=10) == (0 if cas.phase == "arret-ordonne" else -9)
                finally:
                    if vm.poll() is None:
                        vm.kill()
                        vm.wait()
    texte = trace.read_text()
    assert "virtio_iommu_report_fault " not in texte
    assert re.findall(r"virtio_iommu_attach domain=(\d+) endpoint=(\d+)", texte) == [("2","48"),("1","56")]
    stop = plan.pause+int(plan.arret == "write_aio")
    requetes = [(lba*(s//512), s//512) for _, lba, _ in plan.ops[:stop] if lba != FLUSH]
    vus = [(int(a),int(b)) for a,b in re.findall(r"virtio_blk_handle_write .* sector (\d+) nsectors (\d+)", texte)]
    assert vus == requetes, (cas.nom, vus, requetes)
    return ecrits, synchronises, cache, geometrie


def essai(cas, lanceur, sortie, corpus):
    plan = Plan(cas, corpus)
    with tempfile.TemporaryDirectory(dir=sortie) as travail:
        travail = Path(travail)
        cible, args = plan.preparer(travail)
        try:
            e, f, cache, geo = virtualiser(lanceur, sortie, travail, sortie, plan, cible, args)
            brut = travail / "retour.raw"
            if cas.fmt == "qcow2":
                subprocess.run(["qemu-img", "check", "-q", "-f", "qcow2", cible], check=True)
            convertir(cible, cas.fmt, brut, "raw")
            etat, digest, fichier = plan.relire(brut)
        except Exception:
            cible.rename(sortie / f"{cas.nom}.inattendu.{cas.fmt}")
            raise
    print(cas.nom, etat, flush=True)
    return dict(cas=cas.nom, secteur=cas.secteur, format=cas.fmt, op=cas.operation, slot=plan.slot,
                decalage=cas.decalage, plage=cas.plage, phase=cas.phase, arret=plan.arret,
                ecritures=e, flush=f, etat=etat, sha256=digest, fichier=fichier, cache=cache, geometrie=geo)


def disques_systeme(lanceur, sortie):
    """Publication CLI distribuée acquittée, SIGKILL, puis démarrage sur le seul disque."""
    with tarfile.open(ARCHIVE) as archive:
        inst = sortie / "installateur.qcow2"
        inst.write_bytes(archive.extractfile("novaos-0.15.0-utm-aarch64/installateur.qcow2").read())
    source = sortie / "installateur.raw"
    convertir(inst, "qcow2", source, "raw")
    initial = empreinte(source)
    essais = []
    for s in (512, 4096):
        dossier = sortie / f"systeme-{s}"
        dossier.mkdir()
        (dossier / "tpm").mkdir()
        for banque in ("code.fd", "vars.fd"):
            shutil.copyfile(sortie / banque, dossier / banque)
        guids = dossier / "guids.bin"
        guids.write_bytes(b"".join(uuid.uuid4().bytes_le for _ in range(3)))
        taille = (64 if s == 512 else 268) * MIO
        neuf = dossier / "installe.raw"
        with neuf.open("xb") as f:
            f.truncate(taille)

        def installer(vm, canal, serie, trace):
            attendre(vm, serie, PROMPT.encode())
            avant = compteurs(vm)
            assert avant["cible"]["rd_bytes"] == taille and all(v["wr_bytes"] == 0 for v in avant.values())
            canal.sendall(f"AUTORISER {SERIE} {taille}\r".encode())
            attendre(vm, serie, b"INSTALLATION bilan Ok(())", timeout=900)
            assert compteurs(vm)["source"]["wr_bytes"] == 0
            return {}

        machine(lanceur, dossier, "installation", source, neuf, s, guids, installer)
        volume = verifier_gpt(neuf, s, guids.read_bytes())
        controle_domaines(dossier / "installation.trace", installation=True, secteur=s, volume=volume)
        for fmt in ("raw", "qcow2"):
            disque = dossier / f"autonome.{fmt}"
            convertir(neuf, "raw", disque, fmt)
            nom = f"{fmt}-confirme"
            # L'application UTM laisse le modèle choisir sa géométrie sur le profil livré.
            taille_explicite = None if (fmt, s) == ("qcow2", 512) else s

            def publier(vm, canal, serie, trace):
                attendre(vm, serie, b"nova> ")
                bloc = next(d["inserted"] for d in qmp(vm, "query-block") if d["device"] == "cible")
                assert bloc["drv"] == fmt and bloc["cache"] == {"writeback": True, "direct": False, "no-flush": False}
                geometrie = [qmp(vm, "qom-get", path="/machine/peripheral/destination", property=champ)
                             for champ in ("logical_block_size", "physical_block_size")]
                assert geometrie == [s, s]
                position = trace.stat().st_size
                start = serie.stat().st_size
                avant = compteurs(vm)["cible"]
                canal.sendall(b"cp /systeme /audit\r")
                reponse = attendre(vm, serie, b"nova> ", start)
                assert b"indisponible" not in reponse and b"erreur" not in reponse.lower()
                apres = compteurs(vm)["cible"]
                flush = apres["flush_operations"] - avant["flush_operations"]
                assert flush >= 2
                return dict(position=position, flush=flush, cache=bloc["cache"], geometrie=geometrie,
                            cluster=bloc["image"].get("cluster-size"))

            mesures = machine(lanceur, dossier, nom, None, disque, taille_explicite, guids, publier,
                              format_cible=fmt, arret_force=True)
            trace = (dossier / f"{nom}.trace").read_bytes()
            assert b"virtio_iommu_report_fault " not in trace
            assert re.findall(rb"virtio_iommu_attach domain=(\d+) endpoint=(\d+)", trace) == [(b"2", b"56"), (b"1", b"56")]
            debut = volume["p9"] * s // 512
            ancres = [(int(lba)-debut, int(n)) for lba, n in re.findall(
                rb"virtio_blk_handle_write .* sector (\d+) nsectors (\d+)", trace[mesures["position"]:])
                if debut <= int(lba) < debut + 16]
            pas = s // 512
            longueur = 8 // pas
            assert ancres and len(ancres) % longueur == 0
            for i in range(0, len(ancres), longueur):
                slot = ancres[i][0]
                assert slot in (0, 8) and ancres[i:i+longueur] == [(slot+j, pas) for j in range(0, 8, pas)]

            def relire(vm, canal, serie, trace):
                attendre(vm, serie, b"nova> ")
                start = serie.stat().st_size
                canal.sendall(b"cat /audit\r")
                attendre(vm, serie, b"NovaOS\r\nnova> ", start)
                canal.sendall(b"exit\r")
                attendre(vm, serie, b"SYSTEME bilan Ok(())", start)
                return {}

            nom_relecture = f"{fmt}-remontage"
            machine(lanceur, dossier, nom_relecture, None, disque, taille_explicite, guids, relire, format_cible=fmt)
            dma = controle_domaines(dossier / f"{nom_relecture}.trace", installation=False, secteur=s, volume=volume)
            if fmt == "qcow2":
                subprocess.run(["qemu-img", "check", "-q", "-f", fmt, disque], check=True)
            brut = dossier / f"{fmt}-relue.raw"
            convertir(disque, fmt, brut, "raw")
            assert verifier_gpt(brut, s, guids.read_bytes()) == volume
            essais.append(dict(secteur=s, format=fmt, lba_p9=volume["p9"], cache=mesures["cache"],
                               geometrie_qom=mesures["geometrie"], geometrie_explicite=taille_explicite is not None,
                               cluster=mesures["cluster"], flush_cli=mesures["flush"], ecritures_ancres=ancres,
                               signal=9, fichier_relu="/audit", contenu="NovaOS\n", dma_remontage=dma))
    assert empreinte(source) == initial
    return dict(support_brut_sha256=initial, essais=essais)


def exporter_matrice(resultats, fichier):
    with fichier.open("x", newline="") as f:
        lignes = csv.writer(f, delimiter="\t", lineterminator="\n")
        lignes.writerow(("secteur", "format", "opération", "slot", "phase", "décalage_octets",
                         "offset_partiel", "longueur_partielle", "arrêt_backend", "sortie_processus",
                         "OUT_achevées", "FLUSH_achevés", "montage", "sha256_volume_relu"))
        for r in resultats:
            lignes.writerow((r["secteur"], r["format"], r["op"], r["slot"], r["phase"], r["decalage"],
                             *(r["plage"] or ("", "")), r["arret"], 0 if r["phase"] == "arret-ordonne" else -9,
                             r["ecritures"], r["flush"], r["etat"], r["sha256"]))


def verifier_ambiguite(corpus, resultats):
    par_nom = {r["cas"]: r for r in resultats}
    paires = []
    for succes in (r for r in resultats if r["phase"] == "succes"):
        s, fmt, op = succes["secteur"], succes["format"], succes["op"]
        dossier = corpus / f"{s}-{op}"
        prefixe = (Cas(s, fmt, op, "avant-F2", plage=(0, 96)) if op == "mv"
                   else Cas(s, fmt, op, "premier-secteur", decalage=s-96))
        hybride = par_nom[prefixe.nom]
        assert succes["etat"] == "nouveau" and hybride["etat"] == "refus"
        confirme = (dossier / succes["fichier"]).read_bytes()
        avant = (dossier / "avant.raw").read_bytes()
        reference = 4096 * succes["slot"] + 136
        plage = slice(reference, reference + 48)
        # Défaut latent imposé sur une copie en RAM du volume acquitté.
        altere = bytearray(confirme)
        altere[plage] = avant[plage]
        assert altere != confirme and altere == (dossier / hybride["fichier"]).read_bytes()
        paires.append(dict(succes=succes["cas"], prefixe=hybride["cas"], octets=len(altere),
                           sha256_identique=hashlib.sha256(altere).hexdigest()))
    assert len(paires) == 8
    return paires


def main():
    if not __debug__:
        raise RuntimeError("Les contrôles du banc exigent les assertions Python")
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("noyau", type=Path, help="ELF AArch64 construit avec ancres-p9-lab")
    p.add_argument("sortie", type=Path, help="nouveau répertoire sur le stockage à qualifier")
    p.add_argument("--paralleles", type=int, choices=range(1,5), default=2)
    p.add_argument("--filtre", help="sous-ensemble par expression régulière pour diagnostic")
    a = p.parse_args()
    sortie = a.sortie.resolve()
    sortie.mkdir()
    corpus = sortie / "fixtures"
    corpus.mkdir()
    rust(corpus, "aarch64", sortie / "preparation.log")
    assert empreinte(ARCHIVE) == SOMME
    with tarfile.open(ARCHIVE) as archive:
        for nom, banque in (("prototype.fd","code.fd"), ("modele-vars.fd","vars.fd")):
            copie = sortie / nom
            copie.write_bytes(archive.extractfile("novaos-0.15.0-utm-aarch64/"+nom).read())
            pflash(copie, sortie / banque)
    cas = [c for c in matrice() if not a.filtre or re.search(a.filtre, c.nom)]
    assert cas and len(cas) == len({c.nom for c in cas})
    with tempfile.TemporaryDirectory(prefix="p9-utm-", dir="/tmp") as court:
        lanceur = Path(court)
        lanceurs(lanceur)
        moteur = subprocess.check_output([lanceur / "qemu-aarch64-softmmu", "--version"], text=True).splitlines()[0]
        utm = subprocess.check_output(["plutil","-extract","CFBundleShortVersionString","raw",
                                      UTM/"Info.plist"], text=True).strip()
        assert (utm, moteur) == ("5.0.8", "QEMU emulator version 10.0.12")
        source = image(sortie/"support", a.noyau.resolve(), "ancres-p9-lab")
        avant = empreinte(source)
        with ThreadPoolExecutor(max_workers=a.paralleles) as pool:
            resultats = list(pool.map(lambda c: essai(c,lanceur,sortie,corpus), cas))
        assert empreinte(source) == avant
        systemes = disques_systeme(lanceur, sortie) if not a.filtre else None
    with (corpus/"relectures.tsv").open("x") as f:
        for r in resultats:
            print(f"{r['secteur']}-{r['op']}", r["fichier"], r["etat"], sep="\t", file=f)
    if not a.filtre:
        for arch in ("aarch64","x86_64"):
            rust(corpus, arch, sortie / f"relecture-{arch}.log")
    doc = dict(utm=utm, moteur=moteur, disque_source_sha256=avant, firmware_sha256=empreinte(sortie/"prototype.fd"),
               noyau_sonde_sha256=empreinte(a.noyau), outil_image=subprocess.check_output(
                   ["qemu-img","--version"], text=True).splitlines()[0], cas=resultats, systemes=systemes,
               historiques_ambigus=verifier_ambiguite(corpus, resultats) if not a.filtre else [])
    (sortie/"resultats.json").write_text(json.dumps(doc,ensure_ascii=False,indent=2)+"\n")
    exporter_matrice(resultats, sortie / "coupures.tsv")


if __name__ == "__main__":
    main()
