"""Contre-lecture facultative sous Windows PE ARM64, amorcé sur ISO en RAM, supports virtuels en lecture seule."""

import argparse
from pathlib import Path
import re
import subprocess
import sys
import time
import zipfile

from demarrage import Qmp, directory
from support import OS, require

sys.path.insert(0, str(OS / "tests/ethernet_v1"))
from port_qemu import qualification

# Le script s'arrête dans WinPE. Ni cible d'installation ni disque inscriptible.
BATCH = r"""@echo off
setlocal
set LOG=
for %%D in (C D E F G H I J K L M N O P Q R S T U V W Y Z) do if exist %%D:\BOOT0008\VALIDES.SHA set LOG=%%D:
if not defined LOG goto refuse
set REF=%~d0
"%~dp0TEMOIN.EXE"
if errorlevel 1 goto refuse
cls
echo.    NOVA5N FAT SUCCESS 24
goto fin
:refuse
cls
echo WINDOWS FAT READ ERROR
:fin
pause >nul
goto fin
"""


def run(out, installer, image, archive, resources):
    out.mkdir(mode=0o700)
    source = out / "cd"
    source.mkdir()
    with zipfile.ZipFile(archive) as z:
        require(len(z.namelist()) == 24, "Références Windows")
        for n in z.namelist():
            require(
                re.fullmatch(
                    r"BOOT000[1-8]/(rapport\.json|JOURNAL\.TXT|VALIDES\.SHA)", n
                ),
                "Nom de référence",
            )
            dest = source / "REF" / n
            dest.parent.mkdir(exist_ok=True, parents=True)
            dest.write_bytes(z.read(n))
    # Le WinPE minimal ne fournit pas fc.exe. Le témoin ARM64 n'importe que cinq appels de Kernel32.
    share = Path(
        subprocess.check_output(["rustc", "--print", "sysroot"], text=True).strip()
    )
    linkers = list(share.glob("lib/rustlib/*/bin/rust-lld"))
    if not linkers:
        import shutil

        linkers = [Path(shutil.which("rust-lld") or "rust-lld")]
    llvm = (
        Path(subprocess.check_output(["brew", "--prefix", "llvm"], text=True).strip())
        / "bin"
    )
    definition = out / "kernel32.def"
    definition.write_text(
        "LIBRARY KERNEL32.dll\nEXPORTS\nGetEnvironmentVariableA\nCreateFileA\nReadFile\nCloseHandle\nExitProcess\n"
    )
    subprocess.run(
        [
            llvm / "llvm-dlltool",
            "-m",
            "arm64",
            "-d",
            definition,
            "-l",
            out / "kernel32.lib",
        ],
        check=True,
    )
    subprocess.run(
        [
            llvm / "clang",
            "--target=aarch64-pc-windows-msvc",
            "-O1",
            "-ffreestanding",
            "-fno-stack-protector",
            "-c",
            Path(__file__).with_name("lecture_winpe.c"),
            "-o",
            out / "lecture.obj",
        ],
        check=True,
    )
    subprocess.run(
        [
            linkers[0],
            "-flavor",
            "link",
            "/entry:entree",
            "/subsystem:console",
            "/timestamp:0",
            "/nodefaultlib",
            "/out:" + str(source / "TEMOIN.EXE"),
            str(out / "lecture.obj"),
            str(out / "kernel32.lib"),
        ],
        check=True,
    )
    (source / "LIRE.CMD").write_bytes(BATCH.replace("\n", "\r\n").encode("ascii"))
    # Le seul passage est windowsPE. Le test ne partitionne et n'installe rien.
    command = 'cmd.exe /c "for %D in (C D E F G H I J K L M N O P Q R S T U V W Y Z) do @if exist %D:\\LIRE.CMD call %D:\\LIRE.CMD"'
    (source / "Autounattend.xml").write_text(
        '<?xml version="1.0" encoding="utf-8"?><unattend xmlns="urn:schemas-microsoft-com:unattend" '
        'xmlns:wcm="http://schemas.microsoft.com/WMIConfig/2002/State"><settings pass="windowsPE">'
        '<component name="Microsoft-Windows-Setup" processorArchitecture="arm64" publicKeyToken="31bf3856ad364e35" '
        'language="neutral" versionScope="nonSxS"><RunSynchronous><RunSynchronousCommand wcm:action="add">'
        "<Order>1</Order><Path>"
        + command
        + "</Path></RunSynchronousCommand></RunSynchronous></component></settings></unattend>"
    )
    subprocess.run(
        [
            "xorriso",
            "-as",
            "mkisofs",
            "-quiet",
            "-J",
            "-r",
            "-o",
            str(out / "temoin.iso"),
            str(source),
        ],
        check=True,
        stdout=subprocess.DEVNULL,
    )
    for path in (
        installer,
        image,
        resources / "edk2-aarch64-code.fd",
        resources / "edk2-arm-vars.fd",
    ):
        require(path.is_file() and "," not in str(path), "Entrée ordinaire attendue")
    with qualification(Path(__file__).resolve().parents[4], {}), directory(out):
        args = [
            "qemu-system-aarch64",
            "-machine",
            "virt,highmem=off",
            "-accel",
            "hvf",
            "-cpu",
            "host",
            "-smp",
            "4",
            "-m",
            "3G",
            "-nodefaults",
            "-no-reboot",
            "-display",
            "none",
            "-monitor",
            "none",
            "-qmp",
            "stdio",
            "-serial",
            "file:serie.log",
            "-nic",
            "none",
            "-drive",
            "if=pflash,unit=0,format=raw,readonly=on,file="
            + str(resources / "edk2-aarch64-code.fd"),
            "-drive",
            "if=pflash,unit=1,format=raw,snapshot=on,file="
            + str(resources / "edk2-arm-vars.fd"),
            "-device",
            "ramfb",
            "-device",
            "qemu-xhci,id=xhci",
            "-device",
            "usb-kbd,bus=xhci.0",
        ]
        for i, (name, src, kind) in enumerate(
            (
                ("winpe", installer, "cdrom"),
                ("preuve", image, "disk"),
                ("reference", out / "temoin.iso", "cdrom"),
            )
        ):
            args += [
                "-drive",
                f"id={name},if=none,media={kind},format=raw,readonly=on,file={src}",
                "-device",
                "usb-storage,bus=xhci.0,drive="
                + name
                + (",bootindex=1" if i == 0 else ""),
            ]
        with (
            open("qemu.log", "wb") as err,
            subprocess.Popen(
                args,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=err,
                bufsize=0,
            ) as proc,
        ):
            try:
                q = Qmp(proc)
                for tick in range(160):
                    if tick < 4:
                        q.call(
                            "human-monitor-command", **{"command-line": "sendkey spc"}
                        )
                    # Le média de référence peut ne pas être exploré automatiquement par Setup.
                    # La console WinPE ne touche que le CD témoin et la clef virtuelle.
                    if tick in (13, 45):
                        q.call(
                            "human-monitor-command",
                            **{"command-line": "sendkey shift-f10"},
                        )
                    if tick in (16, 48):
                        for letter in "edfg":
                            for colon in ("shift-semicolon", "dot"):
                                for key in [letter, colon, *"lire", "ret"]:
                                    q.call(
                                        "human-monitor-command",
                                        **{"command-line": "sendkey " + key + " 45"},
                                    )
                                    time.sleep(0.10)
                    time.sleep(2)
                    if tick % 3 == 0:
                        q.call("screendump", filename="ecran.ppm")
                        screen = subprocess.run(
                            ["tesseract", "ecran.ppm", "stdout"],
                            capture_output=True,
                            text=True,
                        )
                        Path("ecran.txt").write_text(screen.stdout)
                        if re.search(r"\bFAT SUCCESS 24\b", screen.stdout.upper()):
                            import hashlib
                            import json

                            return json.dumps(
                                dict(
                                    lecteur="Windows PE ARM64",
                                    fichiers_identiques=24,
                                    methode="ReadFile et comparaison octet par octet avec le CD de référence",
                                    archive_sha256=hashlib.sha256(
                                        archive.read_bytes()
                                    ).hexdigest(),
                                    temoin_sha256=hashlib.sha256(
                                        (source / "TEMOIN.EXE").read_bytes()
                                    ).hexdigest(),
                                    capture_ppm_sha256=hashlib.sha256(
                                        Path("ecran.ppm").read_bytes()
                                    ).hexdigest(),
                                    ecran=screen.stdout.strip(),
                                ),
                                ensure_ascii=False,
                                indent=2,
                            )
                raise TimeoutError("Aucune attestation de lecture WinPE")
            finally:
                proc.terminate()
                proc.wait(timeout=10)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    for name in ("iso", "image", "rapports", "ressources", "travail"):
        p.add_argument(name, type=Path)
    a = p.parse_args()
    print(
        run(
            a.travail.resolve(),
            a.iso.resolve(),
            a.image.resolve(),
            a.rapports.resolve(),
            a.ressources.resolve(),
        )
    )
