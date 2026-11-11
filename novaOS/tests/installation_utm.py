"""Reproduire l'audit du démarrage UTM v0.15 sur des fichiers jetables, sans installateur."""
import hashlib
import json
import os
from pathlib import Path
import re
import select
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from session_cli import fixture_p9

UTM = Path("/Applications/UTM.app/Contents")
FIRMWARE = UTM / "Resources/qemu"
MACHINE = "virt-10.0,gic-version=2"
BANNER = b"NovaOS v0.14.0\r\n"


def empreinte(path):
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def lanceurs(tmp):
    # Points d'entrée des bibliothèques livrées par UTM (QEMULauncher/Bootstrap.c).
    # Ces exécutables hôtes permettent de qualifier le moteur installé sans importer de VM.
    sources = {
        "qemu-aarch64-softmmu": """
extern void qemu_init(int, char **, char **);
extern void qemu_main_loop(void);
extern void qemu_cleanup(void);
int main(int argc, char **argv, char **envp) {
    qemu_init(argc, argv, envp);
    qemu_main_loop();
    qemu_cleanup();
    return 0;
}
""",
        "swtpm.0": """
extern int swtpm_main(int, const char **, const char *, const char *);
int main(int argc, const char **argv) { return swtpm_main(argc, argv, "swtpm", "socket"); }
""",
    }
    frameworks = UTM / "Frameworks"
    for name, source in sources.items():
        subprocess.run(["clang", "-x", "c", "-", f"-F{frameworks}", "-framework", name,
                        f"-Wl,-rpath,{frameworks}", "-o", tmp / name], input=source,
                       text=True, check=True)


def observation(cmd, profil, traces, lecture):
    start = time.monotonic()
    # Le témoin négatif observe la VM après l'erreur de traduction émise par le modèle.
    limite = 30 if profil.endswith("disque-protege") else 75
    sortie = bytearray()
    invite = None
    pic_rss = 0
    releve = start
    lu = False
    with (traces / (profil + ".serial")).open("xb") as log, subprocess.Popen(
        cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, bufsize=0
    ) as vm:
        try:
            while time.monotonic() - start < limite:
                if vm.poll() is None and time.monotonic() >= releve:
                    rss = subprocess.run(["ps", "-o", "rss=", "-p", str(vm.pid)], capture_output=True)
                    if rss.stdout.strip().isdigit():
                        pic_rss = max(pic_rss, int(rss.stdout))
                    releve = time.monotonic() + 0.25
                if not select.select([vm.stdout], [], [], 0.25)[0]:
                    continue
                paquet = os.read(vm.stdout.fileno(), 65536)
                if not paquet:
                    break
                log.write(paquet)
                sortie.extend(paquet)
                if lecture and invite is None and b"nova> " in sortie:
                    invite = time.monotonic() - start
                    vm.stdin.write(b"cat /documents/exemple.txt\n")
                if lecture and not lu and b"\r\nNovaOS\r\nnova> " in sortie:
                    lu = True
                    vm.stdin.write(b"exit\n")
                if (b"BLOC session terminee ok=true\r\n" in sortie or
                        re.search(rb"BLOC ouverture refusee \w+\r\n", sortie)):
                    break
        finally:
            if vm.poll() is None:
                vm.kill()
            vm.wait()
    duree = time.monotonic() - start
    if lecture:
        assert lu and b"BLOC session terminee ok=true" in sortie and b"niveau=EL0" in sortie
    if profil.endswith("disque-protege"):
        assert duree >= limite and b"virtio_iommu_translate sid=48 is not known" in sortie
        assert BANNER not in sortie
    if profil == "utm-cd":
        assert b"BLOC ouverture refusee Route" in sortie and BANNER in sortie
        assert re.search(rb"dev=\s*48, type=1 start=0x8020000 end=0x8020fff", sortie)
    if profil == "utm-disque-temoin":
        assert BANNER in sortie and b"BLOC ouverture refusee Feature" in sortie
        assert b"Pci(0x6,0x0)" in sortie
    libres = re.search(rb"(\d+) pages libres", sortie)
    return dict(profil=profil, secondes=round(duree, 3),
                secondes_invite=round(invite, 3) if invite is not None else None,
                cadres_libres_initialisation=int(libres[1]) if libres else None,
                pic_rss_emulateur_kio=pic_rss or None)


def essai(tmp, traces, profil):
    utm = profil.startswith("utm-")
    cd = profil.endswith("-cd")
    protege = profil != "utm-disque-temoin"
    emulateur = tmp / "qemu-aarch64-softmmu" if utm else "qemu-system-aarch64"
    carte = MACHINE + (",its=off" if utm else ",msi=off")
    with tempfile.TemporaryDirectory(dir=tmp) as dossier:
        etat = Path(dossier)
        socket = etat / "tpm"
        with (traces / (profil + ".tpm")).open("xb") as journal, subprocess.Popen(
            [tmp / "swtpm.0", "--tpm2", "--tpmstate", f"dir={etat}", "--ctrl",
             f"type=unixio,path={socket}", "--terminate"], stderr=journal, stdout=journal,
        ) as tpm:
            try:
                deadline = time.monotonic() + 5
                while not socket.exists():
                    if time.monotonic() >= deadline or tpm.poll() is not None:
                        raise RuntimeError(f"TPM indisponible, consulter {journal.name}")
                    time.sleep(0.04)
                args = [emulateur, "-L", FIRMWARE, "-machine", carte, "-accel", "tcg",
                        "-cpu", "cortex-a72", "-smp", "1", "-m", "256M", "-nic", "none",
                        "-display", "none", "-device", "ramfb", "-rtc", "base=utc,clock=host",
                        "-serial", "stdio", "-monitor", "none", "-no-reboot",
                        "-drive", f"if=pflash,unit=0,format=raw,readonly=on,file={FIRMWARE}/edk2-aarch64-code.fd",
                        "-drive", f"if=pflash,unit=1,format=raw,snapshot=on,file={tmp}/vars.fd",
                        "-chardev", f"socket,id=tch,path={socket}", "-tpmdev", "emulator,id=tpm0,chardev=tch",
                        "-device", "tpm-tis-device,tpmdev=tpm0", "-trace", "virtio_iommu_fill_resv_property"]
                image = tmp / "cli.iso"
                if cd:
                    args += ["-device", "virtio-scsi-pci,addr=1", "-drive",
                             f"if=none,id=cd,file={image},format=raw,readonly=on,media=cdrom",
                             "-device", "scsi-cd,drive=cd,bootindex=1"]
                    image = tmp / "volume.raw"
                disque = ("virtio-blk-pci,addr=6,drive=d,disable-legacy=on,ats=off,"
                          "num-queues=1,vectors=0,logical_block_size=512,physical_block_size=512")
                if protege:
                    args += ["-device", "virtio-iommu-pci,addr=5,granule=4k,aw-bits=48,boot-bypass=off"]
                    disque += ",iommu_platform=on"
                if not cd:
                    disque += ",bootindex=1"
                args += ["-drive", f"if=none,id=d,format=raw,snapshot=on,file={image},cache=writeback,"
                         "werror=report,rerror=report", "-device", disque]
                return observation(args, profil, traces, profil == "qemu-cd")
            finally:
                if tpm.poll() is None:
                    tpm.terminate()
                tpm.wait(timeout=5)


def main():
    # Chemin de socket court sur Darwin. Aucun argument ne désigne un disque à écrire.
    with tempfile.TemporaryDirectory(prefix="nova-uefi-", dir="/tmp") as temp:
        tmp = Path(temp)
        lanceurs(tmp)
        dossier_build = ROOT / "build"
        dossier_build.mkdir(exist_ok=True)
        traces = Path(tempfile.mkdtemp(prefix="audit-utm-", dir=dossier_build))
        print(f"Journaux : {traces}", flush=True)
        moteur = tmp / "qemu-aarch64-softmmu"
        versions = [subprocess.check_output([exe, "--version"], text=True).splitlines()[0]
                    for exe in (moteur, "qemu-system-aarch64")]
        option = subprocess.run([moteur, "-machine", MACHINE + ",msi=off", "-display", "none", "-S"],
                                capture_output=True, timeout=10)
        assert option.returncode and b"Property 'virt-10.0-machine.msi' not found" in option.stderr
        (traces / "option-msi.txt").write_bytes(option.stderr)
        subprocess.run(["qemu-img", "convert", "-f", "qcow2", "-O", "raw",
                        FIRMWARE / "edk2-arm-vars.fd", tmp / "vars.fd"], check=True)
        subprocess.run(["sh", ROOT / "scripts/iso.sh", "cli", tmp / "cli.iso"],
                       env={**os.environ, "NOVA_ARCH": "aarch64"}, check=True)
        fixture_p9(tmp / "volume.raw", 512)
        supports = [tmp / "cli.iso", tmp / "volume.raw", tmp / "vars.fd"]
        avant = [empreinte(p) for p in supports]
        resultats = []
        for profil in ("utm-cd", "qemu-cd", "utm-disque-temoin", "utm-disque-protege", "qemu-disque-protege"):
            bilan = essai(tmp, traces, profil)
            print(json.dumps(bilan, ensure_ascii=False), flush=True)
            resultats.append(bilan)
        assert avant == [empreinte(p) for p in supports]
        (traces / "resultats.json").write_text(json.dumps(
            dict(moteurs=versions, empreintes=dict(zip([p.name for p in supports], avant)),
                 essais=resultats), indent=2) + "\n")


if __name__ == "__main__":
    main()
