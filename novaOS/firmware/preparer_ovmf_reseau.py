"""Prépare uniquement une copie de laboratoire d'EDK II pour le profil Q35 réseau."""
import argparse
from pathlib import Path
import subprocess

REVISION = "fbe0805b2091393406952e84724188f8c1941837"
PCI = "MdeModulePkg/Bus/Pci/PciBusDxe/"
# Contexte amont BSD-2-Clause-Patent, voir ../licenses/edk2.txt.
CHANGES = {
    "PciBus.h": (
        "#define EFI_PCI_RID(Bus, Device, Function)",
        """// This root slot is reserved for the OS on the laboratory Q35 machine.
BOOLEAN
NovaNetReservedSlot (
  IN CONST PCI_IO_DEVICE  *Device
  );

#define EFI_PCI_RID(Bus, Device, Function)"""),
    "PciEnumeratorSupport.c": (
        "    PciTestSupportedAttribute (PciIoDevice, &Command,",
        """    if (NovaNetReservedSlot (PciIoDevice)) {
      PCI_READ_COMMAND_REGISTER (PciIoDevice, &OldCommand);
      if ((OldCommand & EFI_PCI_COMMAND_BUS_MASTER) != 0) {
        return EFI_SECURITY_VIOLATION;
      }

      Command &= (UINT16)~EFI_PCI_COMMAND_BUS_MASTER;
    }

    PciTestSupportedAttribute (PciIoDevice, &Command,"""),
    "PciIo.c": (
        "  switch (Operation) {\n    case EfiPciIoAttributeOperationGet:",
        """  if (NovaNetReservedSlot (PciIoDevice) &&
      ((Operation == EfiPciIoAttributeOperationSet) ||
       (Operation == EfiPciIoAttributeOperationEnable)) &&
      ((Attributes & EFI_PCI_IO_ATTRIBUTE_BUS_MASTER) != 0))
  {
    return EFI_ACCESS_DENIED;
  }

  switch (Operation) {
    case EfiPciIoAttributeOperationGet:"""),
}
SLOT = """
BOOLEAN
NovaNetReservedSlot (
  IN CONST PCI_IO_DEVICE  *Device
  )
{
  return (BOOLEAN)((Device->Parent != NULL) &&
                   (Device->PciRootBridgeIo->SegmentNumber == 0) &&
                   (Device->BusNumber == 0) && (Device->DeviceNumber == 7));
}
"""


def prepare(repo):
    def git(*args):
        return subprocess.check_output(["git", "-C", repo, *args])

    if git("rev-parse", "HEAD").decode().strip() != REVISION:
        raise ValueError("Révision EDK II inconnue")
    changes = {}
    for name, (anchor, replacement) in CHANGES.items():
        path = repo / PCI / name
        original = git("show", f"HEAD:{PCI}{name}")
        source = original.replace(b"\r\n", b"\n").decode()
        if source.count(anchor) != 1:
            raise ValueError(f"Contexte inattendu dans {name}")
        result = source.replace(anchor, replacement)
        if name == "PciEnumeratorSupport.c":
            result += SLOT
        data = result.encode()
        if b"\r\n" in original:
            data = data.replace(b"\n", b"\r\n")
        if path.read_bytes() not in (original, data):
            raise ValueError(f"Modifications locales dans {name}")
        changes[path] = data
    for path, data in changes.items():
        if path.read_bytes() != data:
            path.write_bytes(data)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("edk2", type=Path)
    prepare(parser.parse_args().edk2.resolve())
