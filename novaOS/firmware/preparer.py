"""Applique le prototype à une copie de travail jetable d’EDK II stable202502."""
import argparse
from pathlib import Path
import shutil
import subprocess

REVISION = "fbe0805b2091393406952e84724188f8c1941837"
MODULE = "ArmVirtPkg/NovaIommuDxe/NovaIommuDxe.inf"


def preparer(root, retrait_suspendu=False, crb=False, sequentiel=False, retrait_transfert=False):
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
    tpm = "SecurityPkg/Library/Tpm2DeviceLibDTpm/Tpm2Ptp.c"
    # Contexte amont de Tpm2Ptp.c, Intel (2015–2021) et Microsoft,
    # sous BSD-2-Clause-Patent. Voir ../licenses/edk2.txt.
    # Profil ArmVirt privé, CRB avec un seul tampon fixe fourni par le périphérique.
    # Les cinq registres de description sont lus avant le premier octet de commande.
    patches[tpm] = [
        ("  UINT32      TpmOutSize;\n  UINT16      Data16;\n  UINT32      Data32;\n  UINT8       RetryCnt;",
         "  UINT32      TpmOutSize;\n  UINT32      CmdSize, RspSize;\n  UINTN       Buffer;\n"
         "  UINT16      Data16;\n  UINT32      Data32;\n  UINT8       RetryCnt;"),
        ("  for (Index = 0; Index < SizeIn; Index++) {\n    MmioWrite8",
         """  Buffer  = (UINTN)CrbReg->CrbDataBuffer;
  CmdSize = MmioRead32 ((UINTN)&CrbReg->CrbControlCommandSize);
  RspSize = MmioRead32 ((UINTN)&CrbReg->CrbControlResponseSize);
  if ((MmioRead32 ((UINTN)&CrbReg->CrbControlCommandAddressLow) != (UINT32)Buffer) ||
      (MmioRead32 ((UINTN)&CrbReg->CrbControlCommandAddressHigh) != (UINT32)RShiftU64 (Buffer, 32)) ||
      (MmioRead32 ((UINTN)&CrbReg->CrbControlResponseAddrss) != (UINT32)Buffer) ||
      (MmioRead32 ((UINTN)&CrbReg->CrbControlResponseAddrss + 4) != (UINT32)RShiftU64 (Buffer, 32)) ||
      (CmdSize != sizeof (CrbReg->CrbDataBuffer)) || (RspSize != sizeof (CrbReg->CrbDataBuffer)) ||
      (SizeIn < sizeof (TPM2_COMMAND_HEADER)) || (SizeIn > CmdSize) || (*SizeOut < sizeof (TPM2_RESPONSE_HEADER))) {
    Status = EFI_DEVICE_ERROR;
    goto GoIdle_Exit;
  }

  for (Index = 0; Index < SizeIn; Index++) {
    MmioWrite8"""),
        ("""  MmioWrite32 ((UINTN)&CrbReg->CrbControlCommandAddressHigh, (UINT32)RShiftU64 ((UINTN)CrbReg->CrbDataBuffer, 32));
  MmioWrite32 ((UINTN)&CrbReg->CrbControlCommandAddressLow, (UINT32)(UINTN)CrbReg->CrbDataBuffer);
  MmioWrite32 ((UINTN)&CrbReg->CrbControlCommandSize, sizeof (CrbReg->CrbDataBuffer));

  MmioWrite64 ((UINTN)&CrbReg->CrbControlResponseAddrss, (UINT32)(UINTN)CrbReg->CrbDataBuffer);
  MmioWrite32 ((UINTN)&CrbReg->CrbControlResponseSize, sizeof (CrbReg->CrbDataBuffer));

""", ""),
        ("  TpmOutSize = SwapBytes32 (Data32);\n  if (*SizeOut < TpmOutSize)",
         "  TpmOutSize = SwapBytes32 (Data32);\n"
         "  if ((TpmOutSize < sizeof (TPM2_RESPONSE_HEADER)) || (TpmOutSize > RspSize)) {\n"
         "    Status = EFI_DEVICE_ERROR;\n    goto GoIdle_Exit;\n  }\n  if (*SizeOut < TpmOutSize)"),
    ]
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
        changes[path] = original if name == tpm and not (crb or sequentiel) else data
    source = Path(__file__).with_name("NovaIommuDxe")
    dest = root / "ArmVirtPkg" / source.name
    dest.mkdir(exist_ok=True)
    for name in ("NovaIommuDxe.inf", "Domain.c", "Mapping.c", "Profil.c", "Nova.h"):
        path = source / name
        if path.name == "Domain.c" and (retrait_suspendu or sequentiel):
            flags = ("#define NOVA_BANC_RETRAIT_SUSPENDU 1\n" if retrait_suspendu else "")
            flags += "#define NOVA_SEQUENTIEL 1\n" if sequentiel else ""
            flags += "#define NOVA_BANC_RETRAIT_TRANSFERT 1\n" if retrait_transfert else ""
            (dest / path.name).write_text(flags + path.read_text())
        else:
            shutil.copyfile(path, dest / path.name)
    for path, data in changes.items():
        path.write_bytes(data)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("edk2", type=Path)
    parser.add_argument("--retrait-suspendu", action="store_true", help="construire le témoin négatif sans réponse UNMAP")
    parser.add_argument("--crb", action="store_true", help="qualifier le tampon fixe du TPM CRB")
    parser.add_argument("--sequentiel", action="store_true", help="CRB et admission du support nommé par le profil")
    parser.add_argument("--retrait-transfert", action="store_true", help="banc séquentiel, retenir le premier retrait à la sortie")
    args = parser.parse_args()
    if args.retrait_transfert and (not args.sequentiel or args.retrait_suspendu):
        parser.error("--retrait-transfert requiert --sequentiel et exclut --retrait-suspendu")
    preparer(args.edk2.resolve(), args.retrait_suspendu, args.crb, args.sequentiel, args.retrait_transfert)
