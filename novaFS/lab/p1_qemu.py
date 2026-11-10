"""Sonde EDK II de coupure, sur une image NovaFS de laboratoire déjà formatée."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import select
import signal
import stat
import struct
import subprocess
import tempfile
import time

BLOCK = 4096
BLOCKS = 320
HERE = Path(__file__).resolve().parent


def compile_probe(work, sector, operations):
    esp = work / "esp"
    boot = esp / "EFI" / "BOOT"
    boot.mkdir(parents=True)
    with (work / "plan.bin").open("wb") as plan:
        for index, payload in operations:
            plan.write(struct.pack("<I", 0xffffffff if index is None else index))
            plan.write(bytes(BLOCK) if index is None else payload)
    (work / "plan.s").write_text(
        '.section .rdata,"dr"\n.balign 4\n.globl novafs_plan\nnovafs_plan:\n.incbin "plan.bin"\n'
    )
    compiler = [os.environ.get("CLANG", "clang"), "--target=x86_64-pc-windows-msvc"]
    subprocess.run([*compiler, "-O2", "-ffreestanding", "-fno-builtin", "-fno-stack-protector",
                    "-mno-red-zone", "-Wall", "-Wextra", "-Werror", f"-DPLAN_SECTOR={sector}",
                    f"-DPLAN_RECORDS={len(operations)}", "-c", HERE / "p1-probe.c", "-o", "probe.obj"],
                   cwd=work, check=True)
    subprocess.run([*compiler, "-c", "plan.s", "-o", "plan.obj"], cwd=work, check=True)
    rustc = ["rustup", "run", "stable", "rustc"]
    sysroot = Path(subprocess.check_output([*rustc, "--print", "sysroot"], text=True).strip())
    version = subprocess.check_output([*rustc, "-vV"], text=True)
    host = next(line.removeprefix("host: ") for line in version.splitlines() if line.startswith("host: "))
    linker = sysroot / "lib" / "rustlib" / host / "bin" / "rust-lld"
    subprocess.run([linker, "-flavor", "link", "/subsystem:efi_application", "/entry:efi_main",
                    "/nodefaultlib", "/machine:x64", f"/out:{boot / 'BOOTX64.EFI'}",
                    "probe.obj", "plan.obj"], cwd=work, check=True)
    return esp


def check_fixture(fd):
    info = os.fstat(fd)
    if not stat.S_ISREG(info.st_mode) or info.st_size != BLOCKS * BLOCK:
        raise ValueError("fichier image ordinaire de 320 blocs requis")
    anchor = os.pread(fd, BLOCK, 0)
    digest = hashlib.sha256(b"NovaFS.P0.meta\0" + anchor[:64] + bytes(32) + anchor[96:]).digest()
    if (anchor[:16] != b"NOVAP0FS" + struct.pack("<IHH", 0, 1, 96)
            or anchor[16:48] != bytes([0x31])*16 + bytes([0x52])*16
            or anchor[48:56] != bytes(8) or anchor[64:96] != digest
            or struct.unpack_from("<I", anchor, 96)[0] != BLOCK
            or struct.unpack_from("<Q", anchor, 104)[0] != BLOCKS):
        raise ValueError("identités, somme ou géométrie de l’ancre du banc incorrectes")


def completed_io(proc, operations):
    """Lire les compteurs du disque VirtIO après la dernière complétion invitée."""
    proc.stdin.write(b'{"execute":"qmp_capabilities"}\n{"execute":"query-blockstats","id":"volume"}\n')
    proc.stdin.flush()
    pending = b""
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        if not select.select([proc.stdout], [], [], max(0, deadline - time.monotonic()))[0]:
            break
        more = os.read(proc.stdout.fileno(), 32768)
        if not more:
            break
        pending += more
        if len(pending) > 1048576:
            raise RuntimeError("réponse QMP trop longue")
        while b"\n" in pending:
            line, pending = pending.split(b"\n", 1)
            message = json.loads(line)
            if "error" in message:
                raise RuntimeError(f"QMP : {message['error']}")
            if message.get("id") != "volume":
                continue
            stats, = [v["stats"] for v in message["return"] if v.get("device") == "volume"]
            flushes = sum(index is None for index, _ in operations)
            writes = len(operations) - flushes
            for name, expected in (("wr_operations", writes), ("wr_bytes", BLOCK * writes),
                                   ("flush_operations", flushes)):
                if stats[name] != expected:
                    raise RuntimeError(f"QMP {name}={stats[name]}, attendu {expected}")
            return {"writes": writes, "flushes": flushes}
    raise TimeoutError("réponse query-blockstats absente")


def cut_vm(image: Path, sector: int, operations: list, cut: int):
    """Rejouer le préfixe via EFI_BLOCK_IO_PROTOCOL, puis tuer le processus QEMU."""
    if sector not in (512, 4096) or not 0 <= cut <= len(operations) <= 67:
        raise ValueError("géométrie, longueur du plan ou point de coupure")
    for block, data in operations:
        if block is None:
            if data != b"":
                raise ValueError("flush avec charge utile")
        elif not 0 <= block < BLOCKS or len(data) != BLOCK:
            raise ValueError("écriture hors image ou bloc incomplet")
    if not stat.S_ISREG(image.lstat().st_mode):
        raise ValueError("image jetable ordinaire requise")
    qemu = shutil.which("qemu-system-x86_64")
    if qemu is None:
        raise FileNotFoundError("qemu-system-x86_64")
    share = Path(qemu).resolve().parent.parent / "share" / "qemu"
    code = Path(os.environ.get("OVMF_CODE", share / "edk2-x86_64-code.fd"))
    variables = Path(os.environ.get("OVMF_VARS", code.parent / "edk2-i386-vars.fd"))
    with tempfile.TemporaryDirectory(prefix="novafs-p1-efi-") as tmp:
        work = Path(tmp)
        esp = compile_probe(work, sector, operations[:cut])
        debug = work / "debug.log"
        # L'unique disque modifiable de la sonde est le descripteur déjà contrôlé.
        flags = os.O_NOFOLLOW | os.O_NONBLOCK
        # QEMU sonde aussi le fichier en O_RDONLY au démarrage.
        with os.fdopen(os.open(image, os.O_RDWR | flags), "r+b") as writable, \
             os.fdopen(os.open(image, os.O_RDONLY | flags), "rb") as readable:
            fd = writable.fileno()
            ro_fd = readable.fileno()
            if not os.path.samestat(os.fstat(fd), os.fstat(ro_fd)):
                raise ValueError("image substituée entre les ouvertures")
            check_fixture(fd)
            # La virgule est doublée dans les valeurs de la syntaxe QEMU keyval.
            def path_arg(path):
                return str(path).replace(",", ",,")

            args = [qemu, "-machine", "q35", "-accel", "tcg", "-smp", "1", "-m", "128M",
                    "-display", "none", "-qmp", "stdio", "-serial", "none", "-nic", "none",
                    "-no-reboot", "-drive", f"if=pflash,unit=0,format=raw,readonly=on,file={path_arg(code)}",
                    "-drive", f"if=pflash,unit=1,format=raw,snapshot=on,file={path_arg(variables)}",
                    "-drive", f"if=none,id=esp,format=raw,readonly=on,file=fat:ro:{path_arg(esp)}",
                    "-device", "virtio-blk-pci,drive=esp,bootindex=1",
                    "-add-fd", f"fd={fd},set=1", "-add-fd", f"fd={ro_fd},set=1", "-drive",
                    "if=none,id=volume,format=raw,cache=writeback,file.driver=file,file.filename=/dev/fdset/1",
                    "-device", f"virtio-blk-pci,drive=volume,write-cache=on,logical_block_size={sector},physical_block_size={sector}",
                    "-debugcon", f"file:{path_arg(debug)}", "-global", "isa-debugcon.iobase=0xe9"]
            with (work / "stderr.log").open("w+") as errors:
                proc = subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=errors, pass_fds=(fd, ro_fd))
                try:
                    deadline = time.monotonic() + 45
                    while time.monotonic() < deadline and proc.poll() is None:
                        message = debug.read_bytes() if debug.exists() else b""
                        if b"NOVAFS-P1-DONE\n" in message:
                            counts = completed_io(proc, operations[:cut])
                            proc.kill()
                            if proc.wait(timeout=10) != -signal.SIGKILL:
                                raise RuntimeError("arrêt QEMU sans SIGKILL")
                            return counts
                        if b"ERREUR" in message:
                            raise RuntimeError(message.decode(errors="replace"))
                        time.sleep(0.01)
                    errors.seek(0)
                    raise RuntimeError(f"arrêt ou expiration de QEMU : {errors.read()}")
                finally:
                    if proc.poll() is None:
                        proc.kill()
                        proc.wait(timeout=10)
                    proc.stdin.close()
                    proc.stdout.close()
