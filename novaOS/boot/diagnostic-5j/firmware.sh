#!/bin/bash
# Construire dans une copie privée de l'arbre source OVMF PIO historique.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
src=$(cd "$1" && pwd)
out=$2
test "$(git -C "$src" rev-parse HEAD)" = fbe0805b2091393406952e84724188f8c1941837
test "$(shasum -a 256 "$src/OvmfPkg/Library/PlatformBootManagerLib/NovaBootPio.c" | cut -d' ' -f1)" = b55c3ee5d7af7a8a1e490eda5b51940aa3622447d1211e65510840b35b1ee0ee
test "$(git -C "$src" diff --binary HEAD -- . ':!Conf' | shasum -a 256 | cut -d' ' -f1)" = b4924261b73a2c10a2057d4ba259b260ae49371298774338d60e5828a85919d6
mkdir "$out"
rsync -a --exclude=.git --exclude=/Build --exclude=/Conf --exclude=__pycache__ "$src/" "$out/"
cd "$out"
python3 - <<'PY'
from pathlib import Path
p = Path("OvmfPkg/Library/PlatformBootManagerLib/NovaBootPio.c")
s = p.read_text()
def edit(a, b):
    global s
    assert s.count(a) == 1
    s = s.replace(a, b)
edit("#include <Protocol/RamDisk.h>", "#include <Protocol/RamDisk.h>\n#include <Protocol/LoadedImage.h>")
edit("  UINT32                    Revision;", """  UINT32                    Revision;
  EFI_LOADED_IMAGE_PROTOCOL *Loaded;
  STATIC UINT8               Receipt[32];""")
edit('  if (!VariableEquals (L"SecureBoot", 1) || !VariableEquals (L"SetupMode", 0)) {',
     '''  if (!VariableEquals (L"SetupMode", 0) ||
      !(VariableEquals (L"SecureBoot", 1) || VariableEquals (L"SecureBoot", 0))) {''')
edit('  Status = QemuFwCfgFindFile ("opt/novaos/boot.fat", &Item, &Size);',
     '''  Report ("NOVA 5J SecureBoot=", VariableEquals (L"SecureBoot", 1));
  Status = QemuFwCfgFindFile ("opt/novaos/epoch", &Item, &Size);
  if (EFI_ERROR (Status) || (Size != sizeof Receipt)) {
    return EFI_SECURITY_VIOLATION;
  }
  IoWrite16 (FW_CFG_IO_SELECTOR, (UINT16)Item);
  IoReadFifo8 (FW_CFG_IO_DATA, Size, Receipt);
  Status = QemuFwCfgFindFile ("opt/novaos/boot.fat", &Item, &Size);''')
edit('        Status = gBS->StartImage (Image, NULL, NULL);',
     '''        Status = gBS->HandleProtocol (Image, &gEfiLoadedImageProtocolGuid, (VOID **)&Loaded);
        if (!EFI_ERROR (Status)) {
          Loaded->LoadOptions = Receipt;
          Loaded->LoadOptionsSize = sizeof Receipt;
          Report ("NOVA 5J transfert=", AsmReadTsc ());
          Status = gBS->StartImage (Image, NULL, NULL);
        }''')
p.write_text(s)
PY
python3 "$here/s3-pio.py"
bash "$here/compiler-edk.sh"
