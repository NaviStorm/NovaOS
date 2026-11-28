"""Appliquer les raccords sur une copie privée du firmware de référence 5J."""

from hashlib import sha256
from pathlib import Path
import argparse
import shutil

HERE = Path(__file__).resolve().parent
X = "MdeModulePkg/Bus/Pci/XhciDxe"
PCI = "MdeModulePkg/Bus/Pci/PciBusDxe"
BDS = "OvmfPkg/Library/PlatformBootManagerLib"
HASHES = {
    "BaseTools/Source/C/GenFw/GenFw.c": "2845b0febf85f05b01fa0166d4e08b8cb19ac7b67baeb66576f0ab17231f8fd8",
    "MdeModulePkg/Universal/BdsDxe/BdsDxe.inf": "ef78f14fdb0396efc09157fc01358c5e4cc4fe9cf6d946ca7670e561a119698c",
    X
    + "/XhciSched.c": "b18b29339e9bec88426c23c7ca8a92d47f4bcbf5fb12e3e392067b4078bc91fb",
    X
    + "/UsbHcMem.c": "5f2baecd2cc7439eea167bfbaf035935d58e50ee8287f03babf859ed59d14c55",
    X + "/Xhci.c": "355de5e2034c9223ea129f362dd21163f6ee55a639e932c9455207393b01f5ae",
    X
    + "/XhciDxe.inf": "29893cbcf4aa63b90f6f8f7aa4d7c0ddd6f7a8e933558bff285f4e020f6cc69a",
    PCI
    + "/PciEnumeratorSupport.c": "de049ec5f31a363442bc37577bf44ff58adadc53f015cf365ad2b6c2dfb70e1c",
    PCI
    + "/PciBusDxe.inf": "4b860d3ec4d41d3584d28505de7deb81dac96fccb1041207eb7c75830680c919",
    BDS
    + "/NovaBootPio.c": "8f25ba79284a36f70dfc16c5ff98b3d3da4b0c26e4720532d61a7bcea1a79683",
    BDS
    + "/PlatformBootManagerLib.inf": "e27bcd66d42fa7fd6bd96919db4be4156fa9529485909ec12a59468e6b3e3f60",
}


def prepare(root):
    root = root.resolve()
    if not root.is_relative_to(HERE.parents[1] / "build") or root.is_symlink():
        raise ValueError("Copie privée attendue dans novaOS/build")
    for name, digest in HASHES.items():
        if sha256((root / name).read_bytes()).hexdigest() != digest:
            raise ValueError(name)

    def edit(name, old, new):
        p = root / name
        s = p.read_text()
        if s.count(old) != 1:
            raise ValueError((name, old))
        p.write_text(s.replace(old, new))

    def debut(f, nom, lines):
        p = root / f
        s = p.read_text()
        a = s.index("\n" + nom + " (")
        b = s.index("\n{\n", a) + 3
        p.write_text(s[:b] + lines + "\n" + s[b:])

    # GenFw dimensionne un répertoire de débogage sur le chemin de l'ELF,
    # avant sa mise à zéro en RELEASE. Stabiliser uniquement ce nom logique.
    edit(
        "BaseTools/Source/C/GenFw/GenFw.c",
        "    if (!ConvertElf(&FileBuffer, &FileLength)) {",
        """    CHAR8 *OriginalName = mInImageName;
    CONST CHAR8 *Workspace = getenv ("WORKSPACE");
    size_t Prefix = Workspace ? strlen (Workspace) : 0;
    if (!Prefix || strncmp (OriginalName, Workspace, Prefix) || strncmp (OriginalName + Prefix, "/Build/", 7)) {
      Error (NULL, 0, 3000, "Invalid", "Unexpected 5Q ELF input");
      goto Finish;
    }
    /* Preserve the RELEASE layout of the qualified OVMF volume. */
    CONST size_t QualifiedPrefixSize = 110;
    CHAR8 *LogicalName = malloc (QualifiedPrefixSize + strlen (OriginalName + Prefix) + 1);
    if (LogicalName == NULL) { Error (NULL, 0, 4001, "Out of memory", NULL); goto Finish; }
    memset (LogicalName, '_', QualifiedPrefixSize);
    strcpy (LogicalName + QualifiedPrefixSize, OriginalName + Prefix);
    mInImageName = LogicalName;
    BOOLEAN Converted = ConvertElf (&FileBuffer, &FileLength);
    mInImageName = OriginalName;
    free (LogicalName);
    if (!Converted) {""",
    )
    bds_module = root / "MdeModulePkg/Universal/BdsDxe/BdsDxe.inf"
    with bds_module.open("a") as stream:
        stream.write(
            "\n[BuildOptions]\n  GCC:*_CLANGDWARF_X64_DLINK_FLAGS = $(WORKSPACE)/Outils5Q/libverification.a\n"
        )
    for folder in (X, PCI, BDS):
        shutil.copyfile(HERE / "Boite5Q.h", root / folder / "Boite5Q.h")
    shutil.copyfile(HERE / "Buffers5Q.h", root / X / "Buffers5Q.h")
    shutil.copyfile(HERE / "Chargeur5Q.c", root / BDS / "NovaBootPio.c")
    for name in (X + "/XhciDxe.inf", PCI + "/PciBusDxe.inf"):
        edit(name, "[Packages]", "[Packages]\n  OvmfPkg/OvmfPkg.dec")
        edit(name, "[LibraryClasses]", "[LibraryClasses]\n  QemuFwCfgLib\n  IoLib")
    edit(
        BDS + "/PlatformBootManagerLib.inf",
        "[Protocols]",
        "[Protocols]\n  gEfiPciIoProtocolGuid\n  gEfiGraphicsOutputProtocolGuid",
    )
    f = PCI + "/PciEnumeratorSupport.c"
    edit(
        f,
        "BOOLEAN\nNovaNetReservedSlot (",
        '#include "Boite5Q.h"\nBOOLEAN\nNovaNetReservedSlot (',
    )
    edit(
        f,
        "  return (BOOLEAN)(Device->Parent != NULL);",
        """  volatile UINT64 *B = QBox ();
  if (B != NULL && B[11] == 1 && Device->Parent != NULL && Device->PciRootBridgeIo->SegmentNumber == 0 &&
      Device->Pci.Hdr.ClassCode[2] == 0x0c && Device->Pci.Hdr.ClassCode[1] == 3 && Device->Pci.Hdr.ClassCode[0] == 0x30 &&
      B[1] == (UINTN)((Device->BusNumber<<8)|(Device->DeviceNumber<<3)|Device->FunctionNumber)) { return FALSE; }
  return (BOOLEAN)(Device->Parent != NULL);""",
    )
    f = X + "/XhciSched.c"
    edit(f, '#include "Xhci.h"', '#include "Xhci.h"\n#include "Buffers5Q.h"')
    for old, new in (
        (
            "UsbHcAllocateMem (Xhc->MemPool, Entries, FALSE)",
            "QStructure (Q_DCBAA, Entries)",
        ),
        ("UsbHcAllocateMem (Xhc->MemPool, Size, TRUE)", "QStructure (Q_EVENT, Size)"),
        ("UsbHcAllocateMem (Xhc->MemPool, Size, FALSE)", "QStructure (Q_ERST, Size)"),
        (
            "UsbHcAllocateMem (Xhc->MemPool, sizeof (TRB_TEMPLATE) * TrbNum, TRUE)",
            "QStructure (Q_RING, sizeof (TRB_TEMPLATE) * TrbNum)",
        ),
        (
            "Status = Xhc->PciIo->Map (Xhc->PciIo, MapOp, Urb->Data, &Len, &PhyAddr, &Map);",
            "QMap (Urb, MapOp == EfiPciIoOperationBusMasterWrite, Len, &PhyAddr, &Map);\n    Status = EFI_SUCCESS;",
        ),
        ("Xhc->PciIo->Unmap (Xhc->PciIo, Urb->DataMap);", "QStop (7);"),
        ("if (MaxScratchpadBufs != 0) {", "if (MaxScratchpadBufs != 0) { QStop (8);"),
    ):
        edit(f, old, new)
    for kind in ("INPUT", "OUTPUT"):
        for suffix in ("", "_64"):
            typ = kind + "_CONTEXT" + suffix
            if kind == "OUTPUT":
                typ = "DEVICE_CONTEXT" + suffix
            edit(
                f,
                f"UsbHcAllocateMem (Xhc->MemPool, sizeof ({typ}), FALSE)",
                f"QStructure (Q_{kind}, sizeof ({typ}))",
            )
    debut(f, "XhcFlushAsyncIntMap", "  QStop (9);")
    with (root / f).open("a") as stream:
        stream.write(
            "\nUINTN NovaConvert(UINTN A,UINTN N,BOOLEAN D) { return QTranslate(A,N,D); }\nVOID NovaComplete(URB *U,EFI_STATUS S) { QDone(U,S); }\n"
        )
    f = X + "/Xhci.c"
    edit(
        f,
        '#include "Xhci.h"',
        '#include "Xhci.h"\nVOID NovaComplete(URB *U, EFI_STATUS S);',
    )
    edit(
        f,
        "  *TransferResult = Urb->Result;",
        "  NovaComplete(Urb,Status);\n  *TransferResult = Urb->Result;",
    )
    f = X + "/UsbHcMem.c"
    edit(
        f,
        '#include "Xhci.h"',
        '#include "Xhci.h"\nUINTN NovaConvert(UINTN A, UINTN N, BOOLEAN D);',
    )
    debut(
        f, "UsbHcGetPciAddrForHostAddr", "  return NovaConvert ((UINTN)Mem,Size,TRUE);"
    )
    debut(
        f, "UsbHcGetHostAddrForPciAddr", "  return NovaConvert ((UINTN)Mem,Size,FALSE);"
    )
    debut(f, "UsbHcInitMemPool", "  return AllocateZeroPool (sizeof (USBHC_MEM_POOL));")
    debut(
        f,
        "UsbHcFreeMem",
        "  return; /* Les prêts restent réservés jusqu’à la fin de l’époque. */",
    )
    debut(f, "UsbHcFreeMemPool", "  FreePool (Pool); return EFI_SUCCESS;")


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("copie", type=Path)
    prepare(p.parse_args().copie)
