"""Report ACPI local à la description des ports du fw_cfg sur le Q35 de laboratoire."""
import difflib
import hashlib
from pathlib import Path
import subprocess
import tarfile

PATH = "hw/i386/fw_cfg.c"
SOURCE_10 = "96c889f9d9f1cdd2413f033378078d95acb3f05b23da2dea31ef9ffd7f6ffc09"
BEFORE = """    assert(fw_cfg_dma_enabled(fw_cfg));
    io_size = ROUND_UP(FW_CFG_CTL_SIZE, 4) + sizeof(dma_addr_t);"""
AFTER = """    io_size = fw_cfg_dma_enabled(fw_cfg) ?
        ROUND_UP(FW_CFG_CTL_SIZE, 4) + sizeof(dma_addr_t) :
        FW_CFG_CTL_SIZE;"""


def modifier(src, out, amont):
    ref = amont / "qemu-10.0.12.tar.xz"
    with ref.open("rb") as f:
        if hashlib.file_digest(f, "sha256").hexdigest() != SOURCE_10:
            raise ValueError("Archive QEMU 10.0.12 inconnue")
    with tarfile.open(ref, "r:xz") as tar:
        reference = tar.extractfile("qemu-10.0.12/" + PATH).read()
    (out / "fw_cfg-reference-10.0.12.c").write_bytes(reference)
    expr = b'object_property_get_bool(obj, "dma_enabled", NULL) ?\n        ROUND_UP(FW_CFG_CTL_SIZE, 4) + sizeof(dma_addr_t) :\n        FW_CFG_CTL_SIZE;'
    if reference.count(expr) != 1:
        raise ValueError("La référence ACPI ne décrit pas les deux modes attendus")
    path = src / PATH
    original = path.read_bytes()
    (out / "fw_cfg-original.c").write_bytes(original)
    if original.count(BEFORE.encode()) != 1:
        raise ValueError("Contexte ACPI QEMU 11.1.3 inattendu")
    updated = original.replace(BEFORE.encode(), AFTER.encode())
    patch = b"".join(difflib.diff_bytes(difflib.unified_diff, original.splitlines(True),
                                        updated.splitlines(True), b"a/hw/i386/fw_cfg.c", b"b/hw/i386/fw_cfg.c"))
    subprocess.run(["patch", "-p1", "-F", "0"], cwd=src, input=patch, check=True)
    if path.read_bytes() != updated:
        raise ValueError("Le report modifie d'autres octets")
    (out / "fw_cfg-pio.c").write_bytes(updated)
    (out / "fw_cfg-pio.diff").write_bytes(patch)
    return hashlib.sha256(patch).hexdigest()
