/** Support de qualification Q35. Compilé exclusivement dans le BDS OVMF privé. */
#include "BdsPlatform.h"
#include <Protocol/RamDisk.h>

#define SUPPORT_BYTES (34 * 1024 * 1024)

STATIC VOID
Report (CONST CHAR8 *Message, UINT64 Value)
{
  INTN Shift;
  while (*Message != 0) {
    IoWrite8 (0xe9, (UINT8)*Message++);
  }
  for (Shift = 60; Shift >= 0; Shift -= 4) {
    IoWrite8 (0xe9, "0123456789abcdef"[(Value >> Shift) & 15]);
  }
  IoWrite8 (0xe9, '\n');
}

// Transcrire la console texte EFI avant ExitBootServices, même sans affichage graphique.
STATIC EFI_TEXT_STRING ConsoleOutput;
STATIC EFI_STATUS EFIAPI
TraceOutput (EFI_SIMPLE_TEXT_OUTPUT_PROTOCOL *This, CHAR16 *String)
{
  CONST CHAR16 *Cursor;
  for (Cursor = String; *Cursor != 0; ++Cursor) {
    if (*Cursor <= 0x7f) {
      IoWrite8 (0xe9, (UINT8)*Cursor);
    }
  }
  return ConsoleOutput (This, String);
}

STATIC BOOLEAN
VariableEquals (CHAR16 *Name, UINT8 Expected)
{
  UINT8 Value;
  UINTN Size = sizeof Value;
  return !EFI_ERROR (gRT->GetVariable (Name, &gEfiGlobalVariableGuid, NULL, &Size, &Value)) &&
         (Size == sizeof Value) && (Value == Expected);
}

STATIC EFI_STATUS
RunFromPio (VOID)
{
  EFI_RAM_DISK_PROTOCOL     *Ram;
  EFI_DEVICE_PATH_PROTOCOL  *Path, *Tail, *File;
  EFI_HANDLE                Disk, Image;
  EFI_STATUS                Status, Cleanup;
  FIRMWARE_CONFIG_ITEM      Item;
  UINTN                     Size;
  VOID                      *Buffer;
  UINT32                    Revision;

  if (!VariableEquals (L"SecureBoot", 1) || !VariableEquals (L"SetupMode", 0)) {
    return EFI_SECURITY_VIOLATION;
  }
  // Lecture de la révision par le port x86 même si le profil est mal configuré.
  IoWrite16 (FW_CFG_IO_SELECTOR, QemuFwCfgItemInterfaceVersion);
  IoReadFifo8 (FW_CFG_IO_DATA, sizeof Revision, &Revision);
  Report ("NOVA PIO fw_cfg=", Revision);
  if (Revision != 1) {
    return EFI_UNSUPPORTED;
  }
  Status = QemuFwCfgFindFile ("opt/novaos/boot.fat", &Item, &Size);
  if (EFI_ERROR (Status) || (Size != SUPPORT_BYTES)) {
    return EFI_BAD_BUFFER_SIZE;
  }
  Status = gBS->LocateProtocol (&gEfiRamDiskProtocolGuid, NULL, (VOID **)&Ram);
  if (EFI_ERROR (Status)) {
    return Status;
  }
  Buffer = AllocatePages (EFI_SIZE_TO_PAGES (Size));
  if (Buffer == NULL) {
    return EFI_OUT_OF_RESOURCES;
  }
  Report ("NOVA PIO base=", (UINTN)Buffer);
  IoWrite16 (FW_CFG_IO_SELECTOR, (UINT16)Item);
  IoReadFifo8 (FW_CFG_IO_DATA, Size, Buffer);
  Report ("NOVA PIO bytes=", Size);
  Status = Ram->Register ((UINTN)Buffer, Size, &gEfiVirtualDiskGuid, NULL, &Path);
  if (EFI_ERROR (Status)) {
    FreePages (Buffer, EFI_SIZE_TO_PAGES (Size));
    return Status;
  }
  Tail = Path;
  Disk = NULL;
  Status = gBS->LocateDevicePath (&gEfiSimpleFileSystemProtocolGuid, &Tail, &Disk);
  if (!EFI_ERROR (Status) && IsDevicePathEnd (Tail)) {
    File = FileDevicePath (Disk, L"\\EFI\\BOOT\\BOOTX64.EFI");
    if (File == NULL) {
      Status = EFI_OUT_OF_RESOURCES;
    } else {
      Image = NULL;
      Status = gBS->LoadImage (FALSE, gImageHandle, File, NULL, 0, &Image);
      FreePool (File);
      Report ("NOVA PIO LoadImage=", Status);
      if (!EFI_ERROR (Status)) {
        if (gST->ConOut != NULL) {
          ConsoleOutput = gST->ConOut->OutputString;
          gST->ConOut->OutputString = TraceOutput;
        }
        Status = gBS->StartImage (Image, NULL, NULL);
        if (ConsoleOutput != NULL) {
          gST->ConOut->OutputString = ConsoleOutput;
        }
        Report ("NOVA PIO StartImage retour=", Status);
      } else if (Image != NULL) {
        gBS->UnloadImage (Image);
      }
    }
  } else {
    Status = EFI_NOT_FOUND;
  }
  // RamDiskDxe conserve le chemin jusqu'au retrait. Garder une copie pour la relecture.
  Tail = DuplicateDevicePath (Path);
  if (Tail == NULL) {
    return EFI_OUT_OF_RESOURCES;
  }
  File = Tail;
  if (Disk != NULL) {
    Cleanup = gBS->DisconnectController (Disk, NULL, NULL);
    if (EFI_ERROR (Cleanup)) {
      FreePool (File);
      return Cleanup;
    }
  }
  Cleanup = Ram->Unregister (Path);
  if (!EFI_ERROR (Cleanup) &&
      (gBS->LocateDevicePath (&gEfiBlockIoProtocolGuid, &Tail, &Disk) == EFI_NOT_FOUND)) {
    FreePages (Buffer, EFI_SIZE_TO_PAGES (Size));
    Report ("NOVA PIO pages rendues=", EFI_SIZE_TO_PAGES (Size));
  }
  FreePool (File);
  return Status;
}

VOID
NovaPioBoot (VOID)
{
  Report ("NOVA PIO terminal=", RunFromPio ());
  CpuDeadLoop ();
}
