"""Enregistrer la renégociation SMI avec les opcodes S3 PIO standard, sur une copie EDK II."""

from pathlib import Path


def remplacer(path, old, new):
    p = Path("OvmfPkg/SmmControl2Dxe", path)
    s = p.read_text()
    if s.count(old) != 1:
        raise ValueError("Révision inattendue de " + path)
    p.write_text(s.replace(old, new))


for nom in ("SmiFeatures.c", "SmiFeatures.h"):
    remplacer(
        nom,
        "SaveSmiFeatures (\n  VOID\n",
        "SaveSmiFeatures (\n  EFI_S3_SAVE_STATE_PROTOCOL *Script\n",
    )
remplacer(
    "SmmControl2Dxe.c", "    SaveSmiFeatures ();", "    SaveSmiFeatures (S3SaveState);"
)
remplacer(
    "SmiFeatures.c",
    '#include "SmiFeatures.h"',
    '#include <IndustryStandard/QemuFwCfg.h>\n#include "SmiFeatures.h"',
)
remplacer(
    "SmiFeatures.c",
    "  Status = QemuFwCfgS3CallWhenBootScriptReady (",
    """  UINT16 Requested = (UINT16)mRequestedFeaturesItem;
  UINT16 Result = (UINT16)mFeaturesOkItem;
  UINT8 One = 1;
  UINT8 Mask = 0xff;
  QemuFwCfgSelectItem (QemuFwCfgItemInterfaceVersion);
  if (QemuFwCfgRead32 () == 1) {
    Status = Script->Write (Script, EFI_BOOT_SCRIPT_IO_WRITE_OPCODE,
               EfiBootScriptWidthUint16, (UINT64)FW_CFG_IO_SELECTOR, (UINTN)1, &Requested);
    if (!EFI_ERROR (Status)) {
      Status = Script->Write (Script, EFI_BOOT_SCRIPT_IO_WRITE_OPCODE,
                 EfiBootScriptWidthFifoUint8, (UINT64)FW_CFG_IO_DATA, (UINTN)8, &mSmiFeatures);
    }
    if (!EFI_ERROR (Status)) {
      Status = Script->Write (Script, EFI_BOOT_SCRIPT_IO_WRITE_OPCODE,
                 EfiBootScriptWidthUint16, (UINT64)FW_CFG_IO_SELECTOR, (UINTN)1, &Result);
    }
    if (!EFI_ERROR (Status)) {
      Status = Script->Write (Script, EFI_BOOT_SCRIPT_IO_POLL_OPCODE,
                 EfiBootScriptWidthUint8, (UINT64)FW_CFG_IO_DATA, &One, &Mask, (UINT64)0);
    }
    if (EFI_ERROR (Status)) {
      CpuDeadLoop ();
    }
    return;
  }
  Status = QemuFwCfgS3CallWhenBootScriptReady (""",
)
