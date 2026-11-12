"""Applique le prototype à une copie de travail jetable d’EDK II stable202502."""
import argparse
from pathlib import Path
import shutil
import subprocess

REVISION = "fbe0805b2091393406952e84724188f8c1941837"
MODULE = "ArmVirtPkg/NovaIommuDxe/NovaIommuDxe.inf"


def preparer(root, retrait_suspendu=False):
    def git(*arguments):
        return subprocess.check_output(["git", "-C", root, *arguments])

    if git("rev-parse", "HEAD").decode().strip() != REVISION:
        raise ValueError("Révision EDK II inattendue")
    host = "MdeModulePkg/Bus/Pci/PciHostBridgeDxe/PciHostBridgeDxe.inf"
    virtio = "OvmfPkg/Virtio10Dxe/Virtio10.c"
    pci = "MdeModulePkg/Bus/Pci/PciBusDxe/PciIo.c"
    patches = {
        "ArmVirtPkg/ArmVirtQemu.dsc": [("  " + host, "  " + MODULE + "\n  " + host)],
        "ArmVirtPkg/ArmVirtQemuFvMain.fdf.inc": [("  INF " + host, "  INF " + MODULE + "\n  INF " + host)],
        host: [("  gEfiCpuArchProtocolGuid\n", "  gEfiCpuArchProtocolGuid AND\n  gEdkiiIoMmuProtocolGuid\n")],
        virtio: [("SetAttributes = (EFI_PCI_IO_ATTRIBUTE_BUS_MASTER |\n"
                  "                   EFI_PCI_IO_ATTRIBUTE_DUAL_ADDRESS_CYCLE);",
                  "SetAttributes = EFI_PCI_IO_ATTRIBUTE_DUAL_ADDRESS_CYCLE;")],
        "OvmfPkg/VirtioBlkDxe/VirtioBlk.c": [
            ("  EFI_STATUS               UnmapStatus;", "  UINT32                   UsedLength;\n"
             "  EFI_STATUS               UnmapStatus;"),
            ("         &Indices,\n         NULL\n         ) == EFI_SUCCESS) &&",
             "         &Indices,\n         &UsedLength\n         ) == EFI_SUCCESS) &&\n"
             "      (UsedLength == (RequestIsWrite ? 1 : BufferSize + 1)) &&"),
        ],
        pci: [("                                 IoMmuAttribute\n                                 );",
               "                                 IoMmuAttribute\n                                 );\n"
               "      if (EFI_ERROR (Status)) {\n"
               "        PciIoDevice->PciRootBridgeIo->Unmap (PciIoDevice->PciRootBridgeIo, *Mapping);\n"
               "        *Mapping = NULL;\n      }"),
              ("    mIoMmuProtocol->SetAttribute (", "    Status = mIoMmuProtocol->SetAttribute ("),
              ("                      0\n                      );\n  }",
               "                      0\n                      );\n"
               "    if (EFI_ERROR (Status)) {\n      return Status;\n    }\n  }")],
    }
    changes = {}
    for name, edits in patches.items():
        original = git("show", "HEAD:" + name)
        line_end = b"\r\n" if b"\r\n" in original else b"\n"
        result = original.replace(b"\r\n", b"\n").decode()
        for old, new in edits:
            if result.count(old) != 1:
                raise ValueError(f"Point d’insertion ambigu dans {name}")
            result = result.replace(old, new)
        data = result.encode().replace(b"\n", line_end)
        path = root / name
        if path.read_bytes() not in (original, data):
            raise ValueError(f"Modification locale dans {path}")
        changes[path] = data
    source = Path(__file__).with_name("NovaIommuDxe")
    dest = root / "ArmVirtPkg" / source.name
    dest.mkdir(exist_ok=True)
    for name in ("NovaIommuDxe.inf", "Domain.c", "Mapping.c", "Nova.h"):
        path = source / name
        if retrait_suspendu and path.name == "Domain.c":
            (dest / path.name).write_text("#define NOVA_BANC_RETRAIT_SUSPENDU 1\n" + path.read_text())
        else:
            shutil.copyfile(path, dest / path.name)
    for path, data in changes.items():
        path.write_bytes(data)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("edk2", type=Path)
    parser.add_argument("--retrait-suspendu", action="store_true", help="construire le témoin négatif sans réponse UNMAP")
    args = parser.parse_args()
    preparer(args.edk2.resolve(), args.retrait_suspendu)
