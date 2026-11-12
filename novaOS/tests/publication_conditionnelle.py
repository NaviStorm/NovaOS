"""R01/R03 de PC1 sur des cibles neuves avec le moteur livré par UTM."""
import argparse
from concurrent.futures import ThreadPoolExecutor
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
import sys
import tarfile
import tempfile
import time
import uuid

from atomicite_ancres import (ARCHIVE, SOMME, FIRMWARE, ROOT, UTM, attendre, compteurs, convertir,
                             disque_sequentiel, empreinte, image, lanceurs, moniteur, pflash, qmp, tpm,
                             verifier_zeros)
sys.path.insert(0, str(ROOT.parent / "novaFS/lab/pc1"))
import lecture as pc

MIO = 1 << 20


def geometrie(s):
    return (61, 64) if s == 512 else (265, 268)


def neuf(brut, s):
    marque = b"NVPC1Q01" + struct.pack("<I", s) + uuid.uuid4().bytes
    base, taille = geometrie(s)
    with brut.open("xb") as f:
        f.truncate(taille * MIO)
        f.write(marque)
        f.seek(base * MIO)
        f.write(pc.octets(pc.ANCIEN))
        f.flush()
        os.fsync(f.fileno())
    return marque


def argument(marque, mode, pause):
    return "name=opt/novaos/ancres-p9-lab,string=PC1:" + (marque + bytes([mode, pause, 0, 0])).hex()


@dataclass(frozen=True)
class Cas:
    s: int
    fmt: str
    nom: str
    pause: int = 255
    blocage: str = ""
    temoin: bool = False

    @property
    def id(self):
        return f"{self.s}-{self.fmt}-{self.nom}"


def liste():
    for s in (512, 4096):
        for fmt in ("raw", "qcow2"):
            for point in range(13):
                yield Cas(s, fmt, f"pause-{point}", point)
            for point, op in ((0, "write_aio"), (1, "write_aio"), (2, "flush_to_disk"),
                              (4, "write_aio"), (5, "write_aio"), (6, "flush_to_disk")):
                yield Cas(s, fmt, f"pendant-{point}", point, op)
            for i in range(5):
                for temoin in (False, True) if i % 2 == 0 else (True, False):
                    yield Cas(s, fmt, ("témoin-" if temoin else "ordonné-") + str(i), temoin=temoin)


def session(cas, cible, marque, lanceur, commun, travail, relecture=False):
    nom = cas.id + ("-relecture" if relecture else "")
    mode = 2 if relecture else int(cas.temoin)
    pause = 255 if relecture else cas.pause
    point = cas.blocage if not relecture else ""
    s = cas.s
    var = travail / (nom + ".fd")
    shutil.copyfile(commun / "vars.fd", var)
    serie = commun / (nom + ".serial")
    trace = commun / (nom + ".trace")
    disque = disque_sequentiel("cible", 7, False)
    disque[1] += f",logical_block_size={s},physical_block_size={s}"
    io = (f"file.driver=blkdebug,file.image.driver=file,file.image.filename={cible}"
          if point else f"file={cible}")
    with tempfile.TemporaryDirectory(dir=lanceur) as court, tpm(lanceur, commun, nom) as sock:
        uart = Path(court) / "uart"
        cmd = [lanceur / "qemu-aarch64-softmmu", "-L", FIRMWARE, "-machine",
               "virt-10.0,gic-version=3,its=off,virtualization=off", "-accel", "tcg",
               "-cpu", "cortex-a72", "-smp", "1", "-m", "256", "-nic", "none", "-device", "ramfb",
               "-display", "none", "-monitor", "none", "-qmp", "stdio", "-S",
               "-chardev", f"socket,id=u,path={uart},server=on,wait=off,logfile={serie}", "-serial", "chardev:u",
               "-chardev", f"socket,id=t,path={sock}", "-tpmdev", "emulator,id=tpm0,chardev=t",
               "-device", "tpm-crb-device,tpmdev=tpm0", "-drive",
               f"if=pflash,unit=0,format=raw,readonly=on,file={commun/'code.fd'}", "-drive",
               f"if=pflash,unit=1,format=raw,file={var}", "-device",
               "virtio-iommu-pci,addr=5,granule=4k,aw-bits=48,boot-bypass=off",
               "-fw_cfg", argument(marque, mode, pause), "-drive",
               f"if=none,id=source,format=raw,readonly=on,file={commun/'support/disque.raw'}",
               *disque_sequentiel("source", 6, True), "-drive",
               f"if=none,id=cible,format={cas.fmt},{io},discard=unmap,detect-zeroes=unmap,rerror=report,werror=report",
               *disque]
        for evenement in ("virtio_blk_handle_write", "virtio_iommu_attach",
                          "virtio_iommu_report_fault"):
            cmd += ["-trace", evenement]
        with trace.open("xb") as log, subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                                       stderr=log, bufsize=0) as vm:
            try:
                qmp(vm, "qmp_capabilities")
                bloc = next(d["inserted"] for d in qmp(vm, "query-block") if d["device"] == "cible")
                assert bloc["drv"] == cas.fmt and bloc["cache"] == {"writeback": True, "direct": False, "no-flush": False}
                assert [qmp(vm, "qom-get", path="/machine/peripheral/destination", property=k)
                        for k in ("logical_block_size", "physical_block_size")] == [s, s]
                with socket.socket(socket.AF_UNIX) as canal:
                    canal.connect(str(uart))
                    qmp(vm, "cont")
                    if pause < 255:
                        attendre(vm, serie, f"PC1 PAUSE {pause}\r\n".encode(), timeout=85)
                        if point:
                            moniteur(vm, f"break {point} pc1")
                            position_trace = trace.stat().st_size
                            canal.sendall(b"G")
                            if point == "write_aio":
                                attendre(vm, trace, b"virtio_blk_handle_write ", position_trace, timeout=10)
                            else:
                                # Laisser le vCPU soumettre le FLUSH avant l'attente HMP synchrone.
                                time.sleep(0.15)
                            moniteur(vm, "wait_break pc1")
                    else:
                        attendre(vm, serie, b"P9LAB fin Ok(())", timeout=85)
                    stats = compteurs(vm)
                    assert stats["source"]["wr_bytes"] == 0
                    if pause < 255:
                        vm.kill()
                    else:
                        qmp(vm, "quit")
                    assert vm.wait(timeout=10) == (-9 if pause < 255 else 0)
            finally:
                if vm.poll() is None:
                    vm.kill()
                    vm.wait()
    journal = trace.read_text()
    assert "virtio_iommu_report_fault " not in journal
    assert re.findall(r"virtio_iommu_attach domain=(\d+) endpoint=(\d+)", journal) == [("2", "48"), ("1", "56")]
    serie = serie.read_text()
    stats = stats["cible"]
    soumis = [(int(a), int(b)) for a, b in re.findall(
        r"virtio_blk_handle_write .* sector (\d+) nsectors (\d+)", journal)]
    base = geometrie(s)[0] * MIO
    adresses = [9, 1] if cas.temoin else [9, 5, 1, 253]
    commandes = [((base + p * pc.PAGE + off) // 512, s // 512)
                 for p in adresses for off in range(0, pc.PAGE, s)]
    if relecture:
        assert soumis == [] and stats["wr_bytes"] == stats["flush_operations"] == 0
        return re.search(r"PC1 RELECTURE ([0-9a-f]{64})", serie)[1]
    assert soumis == commandes[:len(soumis)]
    assert len(soumis) == stats["wr_operations"] + int(point == "write_aio")
    assert stats["wr_bytes"] == stats["wr_operations"] * s
    # Un wait_break réussi place la commande en vol dans le backend fichier.
    acheves = 0 if pause in (0, 1, 2, 11, 12) else 1 if pause in (3, 4, 5, 6, 10) else 2
    activation = re.search(r"PC1 ACTIVATION ticks=(\d+) hz=(\d+)", serie)
    assert bool(activation) == (pause != 11)
    assert stats["flush_operations"] == acheves + int(bool(activation)), (nom, stats)
    flush = acheves + int(point == "flush_to_disk")
    mesure = re.search(r"PC1 (ACK|TEMOIN) g=2 id=([0-9a-f]{64}) ticks=(\d+) hz=(\d+) f1=(\d+) f2=(\d+)", serie)
    assert bool(mesure) == (pause in (9, 255))
    temps = None
    if mesure:
        assert mesure[2] == pc.identifiant(pc.C2).hex() and flush == 2
        assert (mesure[1] == "TEMOIN") == cas.temoin
        temps = [round(int(mesure[i]) * 1e6 / int(mesure[4]), 3) for i in (3, 5, 6)]
    return dict(nom=nom, secteur=s, format=cas.fmt, point=point, signal=9 if pause < 255 else 0,
                out=stats["wr_operations"], out_soumises=len(soumis), flush=acheves, flush_qmp=stats["flush_operations"],
                flush_soumis=flush, flush_activation=int(bool(activation)),
                activation_us=round(int(activation[1])*1e6/int(activation[2]),3) if activation else None,
                ack=bool(mesure) and not cas.temoin, temoin=cas.temoin,
                us_transaction_f1_f2=temps, cluster=bloc["image"].get("cluster-size"))


def acquisition(cible, brut, cas, marque):
    if cas.fmt == "qcow2":
        subprocess.run(["qemu-img", "check", "-q", "-f", "qcow2", cible], check=True)
    convertir(cible, cas.fmt, brut, "raw")
    base, taille = geometrie(cas.s)
    with brut.open("rb") as f:
        assert f.read(len(marque)) == marque
        verifier_zeros(f, base * MIO - len(marque))
        vol = f.read(MIO)
        verifier_zeros(f, (taille - base - 1) * MIO)
        assert f.read(1) == b""
    return vol


def essai(cas, lanceur, commun):
    with tempfile.TemporaryDirectory(dir=commun) as tmp:
        travail = Path(tmp)
        brut = travail / "neuf.raw"
        marque = neuf(brut, cas.s)
        cible = travail / ("cible." + cas.fmt)
        convertir(brut, "raw", cible, cas.fmt)
        try:
            r = session(cas, cible, marque, lanceur, commun, travail)
            vol = acquisition(cible, travail / "apres.raw", cas, marque)
            # Oracle physique distinct du lecteur : seules les OUT tracées peuvent expliquer ces octets.
            attendu = bytearray(pc.octets(pc.ANCIEN))
            adresses = [9, 1] if cas.temoin else [9, 5, 1, 253]
            tranches = [(p, off) for p in adresses for off in range(0, pc.PAGE, cas.s)]
            for p, off in tranches[:r["out"]]:
                a = p * pc.PAGE + off
                attendu[a:a+cas.s] = pc.AJOUTS[p][off:off+cas.s]
            assert vol == attendu, cas.id
            h = hashlib.sha256(vol).hexdigest()
            # Chaque coupure et le premier arrêt ordonné redémarrent aussi le lecteur invité.
            redemarre = cas.pause < 255 or cas.nom == "ordonné-0"
            if redemarre:
                assert session(cas, cible, marque, lanceur, commun, travail, relecture=True) == h
            etat = pc.inspecter(pc.image(vol))
            if r["ack"]:
                assert etat.etat == "sain" and etat.corps == pc.C2
            fichier = commun / (cas.id + ".pc1")
            with fichier.open("xb") as f:
                f.write(vol)
            r.update(sha256=h, etat=etat.etat, rang=etat.rang, redemarre=redemarre, fichier=fichier.name)
            (commun / (cas.id + ".json")).write_text(json.dumps(r, ensure_ascii=False) + "\n")
            print(cas.id, etat.etat, etat.rang, flush=True)
            return r
        except Exception:
            cible.rename(commun / (cas.id + ".inattendu." + cas.fmt))
            raise


def preparer(sortie, noyau):
    assert empreinte(ARCHIVE) == SOMME
    with tarfile.open(ARCHIVE) as ar:
        for nom, banque in (("prototype.fd", "code.fd"), ("modele-vars.fd", "vars.fd")):
            path = sortie / nom
            path.write_bytes(ar.extractfile("novaos-0.15.0-utm-aarch64/" + nom).read())
            pflash(path, sortie / banque)
    return image(sortie / "support", noyau.resolve(), "ancres-p9-lab")


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("noyau", type=Path)
    p.add_argument("sortie", type=Path)
    p.add_argument("--filtre", default=".")
    p.add_argument("--paralleles", type=int, choices=range(1, 5), default=2)
    a = p.parse_args()
    assert __debug__
    sortie = a.sortie.resolve()
    sortie.mkdir()
    source = preparer(sortie, a.noyau)
    avant = empreinte(source)
    with tempfile.TemporaryDirectory(prefix="pc1-", dir="/tmp") as court:
        lanceur = Path(court)
        lanceurs(lanceur)
        moteur = subprocess.check_output([lanceur / "qemu-aarch64-softmmu", "--version"], text=True).splitlines()[0]
        version = subprocess.check_output(["plutil", "-extract", "CFBundleShortVersionString", "raw", UTM/"Info.plist"], text=True).strip()
        assert (moteur, version) == ("QEMU emulator version 10.0.12", "5.0.8")
        cas = [c for c in liste() if re.search(a.filtre, c.id)]
        assert cas
        with ThreadPoolExecutor(max_workers=a.paralleles) as pool:
            resultats = list(pool.map(lambda c: essai(c, lanceur, sortie), cas))
    assert empreinte(source) == avant
    (sortie / "resultats.json").write_text(json.dumps(dict(utm=version, qemu=moteur, source=avant,
        firmware=empreinte(sortie / "prototype.fd"), noyau=empreinte(a.noyau), essais=resultats),
        ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
