/** SPDX-License-Identifier: BSD-2-Clause-Patent
  BDS de laboratoire. La seule source des octets de chargement est le BlockIO
  de la partition USB admise. Les reçus sont ajoutés au suffixe RAM privé. */
#include "BdsPlatform.h"
#include "Boite5Q.h"
#include <Protocol/BlockIo.h>
#include <Protocol/GraphicsOutput.h>
#include <Protocol/LoadedImage.h>
#include <Protocol/RamDisk.h>

#define Q_TAIL 16384U
UINTN EFIAPI nova_verifier_usb(CONST UINT8 *Envelope, UINTN Length,
                               CONST UINT8 *Epoch, CONST UINT64 *Facts,
                               CONST UINT8 *Esp, UINTN EspLength, UINTN Offset,
                               UINTN FileLength, CONST UINT8 *Path,
                               UINTN PathLength);
STATIC EFI_STATUS QBoot(VOID) {
  EFI_HANDLE *Handles, Device = NULL, Image = NULL, Disk = NULL;
  EFI_PCI_IO_PROTOCOL *Pci;
  EFI_BLOCK_IO_PROTOCOL *Block = NULL, *Candidate;
  EFI_DEVICE_PATH_PROTOCOL *Path, *Tail, *File, *Parent, *Walk;
  EFI_RAM_DISK_PROTOCOL *Ram;
  EFI_LOADED_IMAGE_PROTOCOL *Loaded;
  EFI_GRAPHICS_OUTPUT_PROTOCOL *Gop;
  UINTN N, I, Seg, Bus, Dev, Fun, Bytes, Pos, Len, ParentSize, PathSize;
  UINTN UsbNodes, Partitions, Found = 0, BootPort = 0;
  UINT8 *Buffer, Mode = 2, Setup = 2;
  UINT64 Gpa, Facts[16];
  UINT32 BlockSize;
  EFI_STATUS S;
  volatile UINT64 *B = QBox();
  if (!B) {
    return EFI_UNSUPPORTED;
  }
  Len = 1;
  S = gRT->GetVariable(L"SetupMode", &gEfiGlobalVariableGuid, NULL, &Len,
                       &Setup);
  if (EFI_ERROR(S) || Len != 1 || Setup != 0) {
    return EFI_SECURITY_VIOLATION;
  }
  Len = 1;
  S = gRT->GetVariable(L"SecureBoot", &gEfiGlobalVariableGuid, NULL, &Len,
                       &Mode);
  if (EFI_ERROR(S) || Len != 1 || Mode > 1) {
    return EFI_SECURITY_VIOLATION;
  }
  B[23] = Mode;
  S = gBS->LocateHandleBuffer(ByProtocol, &gEfiPciIoProtocolGuid, NULL, &N,
                              &Handles);
  if (EFI_ERROR(S)) {
    return S;
  }
  for (I = 0; I < N; I++) {
    if (EFI_ERROR(gBS->HandleProtocol(Handles[I], &gEfiPciIoProtocolGuid,
                                      (VOID **)&Pci)) ||
        EFI_ERROR(Pci->GetLocation(Pci, &Seg, &Bus, &Dev, &Fun))) {
      return EFI_DEVICE_ERROR;
    }
    if (Seg == 0 && ((Bus << 8) | (Dev << 3) | Fun) == B[1]) {
      if (Device != NULL) {
        return EFI_SECURITY_VIOLATION;
      }
      Device = Handles[I];
    }
  }
  FreePool(Handles);
  if (!Device) {
    return EFI_NOT_FOUND;
  }
  Parent = DevicePathFromHandle(Device);
  if (!Parent) {
    return EFI_NOT_FOUND;
  }
  ParentSize = GetDevicePathSize(Parent) - END_DEVICE_PATH_LENGTH;
  S = gBS->ConnectController(Device, NULL, NULL, TRUE);
  if (EFI_ERROR(S) && S != EFI_ALREADY_STARTED) {
    return S;
  }
  S = gBS->LocateHandleBuffer(ByProtocol, &gEfiBlockIoProtocolGuid, NULL, &N,
                              &Handles);
  if (EFI_ERROR(S)) {
    return S;
  }
  for (I = 0; I < N; I++) {
    Path = DevicePathFromHandle(Handles[I]);
    if (!Path) {
      continue;
    }
    PathSize = GetDevicePathSize(Path);
    if (PathSize <= ParentSize + END_DEVICE_PATH_LENGTH || PathSize > 128 ||
        CompareMem(Parent, Path, ParentSize)) {
      continue;
    }
    Walk = (EFI_DEVICE_PATH_PROTOCOL *)((UINT8 *)Path + ParentSize);
    UsbNodes = 0;
    Partitions = 0;
    while (!IsDevicePathEnd(Walk)) {
      if (DevicePathType(Walk) == MESSAGING_DEVICE_PATH &&
          DevicePathSubType(Walk) == MSG_USB_DP &&
          DevicePathNodeLength(Walk) == sizeof(USB_DEVICE_PATH) &&
          !Partitions) {
        UsbNodes++;
      } else if (DevicePathType(Walk) == MEDIA_DEVICE_PATH &&
                 DevicePathSubType(Walk) == MEDIA_HARDDRIVE_DP &&
                 DevicePathNodeLength(Walk) == sizeof(HARDDRIVE_DEVICE_PATH) &&
                 UsbNodes && !Partitions) {
        Partitions++;
      } else {
        break;
      }
      Walk = NextDevicePathNode(Walk);
    }
    if (!IsDevicePathEnd(Walk) || UsbNodes != 1 || Partitions != 1) {
      continue;
    }
    S = gBS->HandleProtocol(Handles[I], &gEfiBlockIoProtocolGuid,
                            (VOID **)&Candidate);
    if (EFI_ERROR(S) || !Candidate->Media ||
        !Candidate->Media->LogicalPartition) {
      return EFI_DEVICE_ERROR;
    }
    /* La GPT fournit deux partitions; le superviseur nomme le GUID de l'ESP
       après une contre-lecture de l'image en lecture seule. */
    Walk = (EFI_DEVICE_PATH_PROTOCOL *)((UINT8 *)Path + ParentSize);
    Walk = NextDevicePathNode(Walk);
    if (((HARDDRIVE_DEVICE_PATH *)Walk)->SignatureType != SIGNATURE_TYPE_GUID ||
        CompareMem(((HARDDRIVE_DEVICE_PATH *)Walk)->Signature, (VOID *)&B[48],
                   16)) {
      continue;
    }
    BootPort = 1 + (UINTN)((USB_DEVICE_PATH *)((UINT8 *)Path + ParentSize))
                       ->ParentPortNumber;
    Found++;
    Block = Candidate;
    B[16] = PathSize;
    CopyMem((VOID *)&B[32], Path, PathSize);
  }
  FreePool(Handles);
  if (Found != 1 || !Block || !Block->Media->MediaPresent ||
      Block->Media->BlockSize != 512 ||
      Block->Media->LastBlock >= ((64 * 1024 * 1024 - 512) / 512)) {
    return EFI_SECURITY_VIOLATION;
  }
  BlockSize = Block->Media->BlockSize;
  Bytes = ((UINTN)Block->Media->LastBlock + 1) * BlockSize;
  if (Bytes < 1024 * 1024) {
    return EFI_BAD_BUFFER_SIZE;
  }
  Buffer = AllocatePages(EFI_SIZE_TO_PAGES(Bytes + Q_TAIL));
  if (!Buffer) {
    return EFI_OUT_OF_RESOURCES;
  }
  for (Pos = 0; Pos < Bytes; Pos += Len) {
    Len = MIN((UINTN)65536, Bytes - Pos);
    S = Block->ReadBlocks(Block, Block->Media->MediaId, Pos / BlockSize, Len,
                          Buffer + Pos);
    if (EFI_ERROR(S)) {
      return S;
    }
  }
  ZeroMem(Buffer + Bytes, Q_TAIL);
  if (B[8] != B[9] || B[10] || B[2]) {
    return EFI_SECURITY_VIOLATION;
  }
  S = gBS->LocateProtocol(&gEfiRamDiskProtocolGuid, NULL, (VOID **)&Ram);
  if (EFI_ERROR(S)) {
    return S;
  }
  S = Ram->Register((UINTN)Buffer, Bytes + Q_TAIL, &gEfiVirtualDiskGuid, NULL,
                    &Path);
  if (EFI_ERROR(S)) {
    return S;
  }
  Tail = Path;
  S = gBS->LocateDevicePath(&gEfiSimpleFileSystemProtocolGuid, &Tail, &Disk);
  if (EFI_ERROR(S) || !IsDevicePathEnd(Tail)) {
    return EFI_NOT_FOUND;
  }
  File = FileDevicePath(Disk, L"\\EFI\\BOOT\\BOOTX64.EFI");
  if (!File) {
    return EFI_OUT_OF_RESOURCES;
  }
  B[12] = (UINTN)Buffer;
  B[13] = Bytes;
  B[27] = 1;
  MemoryFence();
  B[2] = 7;
  while (B[2] == 7) {
    CpuPause();
  }
  MemoryFence();
  if (B[2] != 8) {
    return EFI_SECURITY_VIOLATION;
  }
  B[2] = 0;
  S = gBS->LoadImage(FALSE, gImageHandle, File, NULL, 0, &Image);
  if (EFI_ERROR(S)) {
    return S;
  }
  S = gBS->HandleProtocol(Image, &gEfiLoadedImageProtocolGuid,
                          (VOID **)&Loaded);
  if (EFI_ERROR(S)) {
    return S;
  }
  B[24] = (UINTN)Loaded->ImageBase;
  B[25] = Loaded->ImageSize;
  S = gBS->LocateProtocol(&gEfiGraphicsOutputProtocolGuid, NULL, (VOID **)&Gop);
  if (EFI_ERROR(S) || !Gop->Mode || !Gop->Mode->Info) {
    return EFI_NOT_FOUND;
  }
  Gpa = Gop->Mode->FrameBufferBase;
  B[17] = Gpa;
  B[18] = Gop->Mode->FrameBufferSize;
  B[19] = Gop->Mode->Info->HorizontalResolution;
  B[20] = Gop->Mode->Info->VerticalResolution;
  B[21] = Gop->Mode->Info->PixelsPerScanLine;
  B[22] = Gop->Mode->Info->PixelFormat;
  B[27] = 2;
  MemoryFence();
  B[2] = 4;
  while (B[2] == 4) {
    CpuPause();
  }
  MemoryFence();
  if (B[2] != 5 || B[10] || B[8] != B[9]) {
    return EFI_SECURITY_VIOLATION;
  }
  Facts[0] = B[1];
  Facts[1] = 1;
  Facts[2] = Mode;
  Facts[3] = BootPort;
  Facts[4] = B[8];
  Facts[5] = B[9];
  Facts[6] = B[24];
  Facts[7] = B[25];
  for (I = 0; I < 6; I++) {
    Facts[8 + I] = B[17 + I];
  }
  Facts[14] = B[14];
  Facts[15] = B[15];
  if (B[26] > 12000 ||
      !nova_verifier_usb(Buffer + Bytes, B[26], (UINT8 *)&B[52], Facts, Buffer,
                         Bytes, B[14], B[15], (UINT8 *)&B[32], B[16])) {
    return EFI_SECURITY_VIOLATION;
  }
  B[29] = 1;
  CopyMem(Buffer + Bytes + Q_TAIL - 344, (VOID *)&B[132], 344);
  /* L'ancre doit être dans la mémoire BootServices décrite par EFI. */
  Loaded->LoadOptions = AllocateCopyPool(32, (VOID *)&B[128]);
  if (!Loaded->LoadOptions) {
    return EFI_OUT_OF_RESOURCES;
  }
  Loaded->LoadOptionsSize = 32;
  B[27] = 3;
  MemoryFence();
  return gBS->StartImage(Image, NULL, NULL);
}
VOID NovaPioBoot(VOID) {
  EFI_STATUS S = QBoot();
  volatile UINT64 *B = QBox();
  if (B) {
    B[28] = S;
    B[2] = 6;
  }
  CpuDeadLoop();
}
