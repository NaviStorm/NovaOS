// SPDX-License-Identifier: MIT
// Banc AArch64 virt-10.0 à un segment PCI et un disque moderne.
#include "Nova.h"

STATIC UINT32 UnitRid, DiskSid;
STATIC EFI_HANDLE DiskHandle;
STATIC EFI_PCI_IO_PROTOCOL *DiskPci, *UnitPci;
STATIC VIRTIO_DEVICE_PROTOCOL *Disk, *Unit;
STATIC VRING Requests, Events;
STATIC UINT8 *Command, *Reply, *Fault;
STATIC BOOLEAN Attached, Starting;

VOID NovaLog (CONST CHAR8 *Format, ...) {
  CHAR8 Text[160];
  VA_LIST Args;
  VA_START (Args, Format);
  AsciiVSPrint (Text, sizeof Text, Format, Args);
  VA_END (Args);
  SerialPortWrite ((UINT8 *)Text, AsciiStrLen (Text));
}

VOID NovaStop (CONST CHAR8 *Reason) {
  DisableInterrupts ();
  // Aucune libération ni nouvelle commande IOMMU après une réponse incertaine.
  // Couper le disque même si le canal de contrôle du domaine ne répond plus.
  if (DiskPci != NULL) {
    UINT16 Cmd;
    if (!EFI_ERROR (DiskPci->Pci.Read (DiskPci, EfiPciIoWidthUint16, 4, 1, &Cmd))) {
      Cmd &= (UINT16)~BIT2;
      DiskPci->Pci.Write (DiskPci, EfiPciIoWidthUint16, 4, 1, &Cmd);
      DiskPci->Pci.Read (DiskPci, EfiPciIoWidthUint16, 4, 1, &Cmd);
      NovaLog ("NOVA DXE arret BME=%u\r\n", (UINT32)((Cmd & BIT2) != 0));
    }
  }
  NovaLog ("NOVA DXE QUARANTAINE %a\r\n", Reason);
  CpuDeadLoop ();
}

VOID NovaCheck (EFI_STATUS Status, CONST CHAR8 *Step) {
  if (EFI_ERROR (Status)) {
    NovaLog ("NOVA DXE %a %r\r\n", Step, Status);
    NovaStop (Step);
  }
}

STATIC EFI_STATUS Cells (FDT_CLIENT_PROTOCOL *Fdt, INT32 Node, CONST CHAR8 *Name, UINT32 *Dest,
                         UINTN Count) {
  CONST VOID *Prop;
  UINT32 Size;
  EFI_STATUS Status = Fdt->GetNodeProperty (Fdt, Node, Name, &Prop, &Size);
  if (EFI_ERROR (Status) || Size != Count * sizeof (UINT32)) {
    return EFI_UNSUPPORTED;
  }
  for (UINTN I = 0; I < Count; ++I) {
    Dest[I] = SwapBytes32 (ReadUnaligned32 ((CONST UINT32 *)Prop + I));
  }
  return EFI_SUCCESS;
}

EFI_STATUS NovaTopology (VOID) {
  FDT_CLIENT_PROTOCOL *Fdt;
  INT32 Root, Node, Other;
  UINT32 Reg[5], Phandle, CellCount, Ranges[8];
  CONST VOID *Prop;
  EFI_STATUS S = gBS->LocateProtocol (&gFdtClientProtocolGuid, NULL, (VOID **)&Fdt);
  if (EFI_ERROR (S)) {
    return S;
  }
  if (EFI_ERROR (Fdt->FindCompatibleNode (Fdt, "pci-host-ecam-generic", &Root)) ||
      Fdt->FindNextCompatibleNode (Fdt, "pci-host-ecam-generic", Root, &Other) != EFI_NOT_FOUND ||
      EFI_ERROR (Fdt->FindCompatibleNode (Fdt, "virtio,pci-iommu", &Node)) ||
      Fdt->FindNextCompatibleNode (Fdt, "virtio,pci-iommu", Node, &Other) != EFI_NOT_FOUND ||
      Fdt->GetNodeProperty (Fdt, Root, "iommu-map-mask", &Prop, NULL) != EFI_NOT_FOUND ||
      Fdt->GetNodeProperty (Fdt, Root, "msi-map", &Prop, NULL) != EFI_NOT_FOUND ||
      EFI_ERROR (Cells (Fdt, Node, "phandle", &Phandle, 1)) || Phandle == 0 ||
      EFI_ERROR (Cells (Fdt, Node, "#iommu-cells", &CellCount, 1)) || CellCount != 1 ||
      EFI_ERROR (Cells (Fdt, Node, "reg", Reg, 5)) ||
      EFI_ERROR (Cells (Fdt, Root, "iommu-map", Ranges, 8))) {
    return EFI_UNSUPPORTED;
  }
  // Ce profil accepte l’identité RID/SID sur tout le segment, à l’exception
  // du contrôleur de confiance. Un masque, un alias ou un trou est refusé.
  UnitRid = Reg[0] >> 8;
  if (UnitRid == 0 || UnitRid >= MAX_UINT16 || Reg[0] != (UnitRid << 8) || Reg[1] || Reg[2] ||
      Reg[3] || Reg[4] || Ranges[0] || Ranges[1] != Phandle || Ranges[2] || Ranges[3] != UnitRid ||
      Ranges[4] != UnitRid + 1 || Ranges[5] != Phandle || Ranges[6] != Ranges[4] ||
      Ranges[7] != 0x10000U - Ranges[4]) {
    return EFI_UNSUPPORTED;
  }
  return EFI_SUCCESS;
}

STATIC UINT32 Id (EFI_PCI_IO_PROTOCOL *Pci) {
  UINT32 Id;
  NovaCheck (Pci->Pci.Read (Pci, EfiPciIoWidthUint32, 0, 1, &Id), "identité PCI");
  return Id;
}

STATIC BOOLEAN IsMaster (EFI_PCI_IO_PROTOCOL *Pci) {
  UINT16 CommandReg;
  NovaCheck (Pci->Pci.Read (Pci, EfiPciIoWidthUint16, 4, 1, &CommandReg), "commande PCI");
  return (CommandReg & BIT2) != 0;
}

STATIC VOID BusMaster (EFI_PCI_IO_PROTOCOL *Pci, BOOLEAN Enable) {
  NovaCheck (Pci->Attributes (
                 Pci, Enable ? EfiPciIoAttributeOperationEnable : EfiPciIoAttributeOperationDisable,
                 EFI_PCI_IO_ATTRIBUTE_BUS_MASTER, NULL),
             "BME");
  MemoryFence ();
  if (IsMaster (Pci) != Enable) {
    NovaStop ("lecture BME");
  }
}

VOID NovaMaster (BOOLEAN Enable) {
  BusMaster (DiskPci, Enable);
}

STATIC EFI_STATUS Identify (EFI_HANDLE Requested) {
  EFI_HANDLE *Handles, Controller = NULL;
  UINTN Count, Disks = 0;
  EFI_STATUS S =
      gBS->LocateHandleBuffer (ByProtocol, &gEfiPciIoProtocolGuid, NULL, &Count, &Handles);
  if (EFI_ERROR (S)) {
    return S;
  }
  for (UINTN I = 0; I < Count; ++I) {
    EFI_PCI_IO_PROTOCOL *Pci;
    UINTN Segment, Bus, Slot, Function;
    NovaCheck (gBS->HandleProtocol (Handles[I], &gEfiPciIoProtocolGuid, (VOID **)&Pci), "PCI");
    NovaCheck (Pci->GetLocation (Pci, &Segment, &Bus, &Slot, &Function), "RID");
    if (Segment != 0 || Bus > 255 || Slot > 31 || Function > 7) {
      gBS->FreePool (Handles);
      return EFI_UNSUPPORTED;
    }
    UINT32 Rid = (UINT32)((Bus << 8) | (Slot << 3) | Function);
    UINT32 PciId = Id (Pci);
    if (Handles[I] == Requested && PciId != 0x10421af4U) {
      NovaLog ("NOVA DXE droit refuse RID=%u\r\n", Rid);
    }
    if (Rid == UnitRid && PciId == 0x10571af4U) {
      Controller = Handles[I];
      UnitPci = Pci;
    }
    if (PciId == 0x10421af4U) {
      ++Disks;
      DiskHandle = Handles[I];
      DiskPci = Pci;
      DiskSid = Rid;
    }
  }
  gBS->FreePool (Handles);
  if (Disks != 1 || Controller == NULL || DiskHandle != Requested || DiskSid == UnitRid) {
    DiskHandle = NULL;
    return EFI_ACCESS_DENIED;
  }
  S = gBS->HandleProtocol (Controller, &gVirtioDeviceProtocolGuid, (VOID **)&Unit);
  if (EFI_ERROR (S)) {
    gBS->ConnectController (Controller, NULL, NULL, FALSE);
    S = gBS->HandleProtocol (Controller, &gVirtioDeviceProtocolGuid, (VOID **)&Unit);
  }
  if (EFI_ERROR (S)) {
    return S;
  }
  return gBS->HandleProtocol (DiskHandle, &gVirtioDeviceProtocolGuid, (VOID **)&Disk);
}

STATIC EFI_STATUS EFIAPI PrivatePages (VIRTIO_DEVICE_PROTOCOL *Unused, UINTN Pages, VOID **Host) {
  EFI_PHYSICAL_ADDRESS Pa = 0;
  EFI_STATUS S = gBS->AllocatePages (AllocateAnyPages, EfiBootServicesData, Pages, &Pa);
  if (!EFI_ERROR (S)) {
    *Host = (VOID *)(UINTN)Pa;
    ZeroMem (*Host, EFI_PAGES_TO_SIZE (Pages));
  }
  return S;
}

STATIC VOID Queue (UINT16 Index, VRING *Ring) {
  UINT16 Size;
  VIRTIO_DEVICE_PROTOCOL Private = *Unit;
  Private.AllocateSharedPages = PrivatePages;
  NovaCheck (Unit->SetQueueSel (Unit, Index), "sélection file");
  NovaCheck (Unit->GetQueueNumMax (Unit, &Size), "taille file");
  if (Size < 2 || Size > 256 || (Size & (Size - 1))) {
    NovaStop ("géométrie file");
  }
  NovaCheck (VirtioRingInit (&Private, Size, Ring), "pages privées");
  // Seul le contrôleur exclu du iommu-map voit ces adresses physiques.
  NovaCheck (Unit->SetQueueAddress (Unit, Ring, 0), "adresse file");
}

STATIC VOID Request (UINT32 InSize, UINT32 OutSize) {
  DESC_INDICES Idx;
  UINT16 Old = *Requests.Avail.Idx;
  UINT16 Next = (UINT16)(Old + 1);
  if (*Events.Used.Idx != 0 || *Requests.Used.Idx != Old) {
    NovaStop ("file occupée ou faute DMA");
  }
  SetMem (Reply, OutSize, 0xff);
  VirtioPrepare (&Requests, &Idx);
  VirtioAppendDesc (&Requests, (UINTN)Command, InSize, VRING_DESC_F_NEXT, &Idx);
  VirtioAppendDesc (&Requests, (UINTN)Reply, OutSize, VRING_DESC_F_WRITE, &Idx);
  Requests.Avail.Ring[Old % Requests.QueueSize] = 0;
  MemoryFence ();
#ifdef NOVA_BANC_RETRAIT_SUSPENDU
  // Témoin négatif : retenir UNMAP avant publication, garder toutes les pages.
  if (Command[0] != 4)
#endif
  {
    *Requests.Avail.Idx = Next;
    MemoryFence ();
    NovaCheck (Unit->SetQueueNotify (Unit, 0), "notification");
  }
  UINT64 Start = GetPerformanceCounter ();
  while (*Requests.Used.Idx != Next) {
    if (GetTimeInNanoSecond (GetPerformanceCounter () - Start) >= 1000000000ULL) {
      NovaStop ("délai commande IOMMU");
    }
    CpuPause ();
  }
  MemoryFence ();
  volatile VRING_USED_ELEM *Used = &Requests.Used.UsedElem[Old % Requests.QueueSize];
  if (Used->Id != 0 || Used->Len != OutSize || !IsZeroBuffer (Reply + OutSize - 4, 4) ||
      *Events.Used.Idx != 0) {
    NovaStop ("acquittement IOMMU");
  }
}

STATIC VOID EndpointCommand (UINT8 Opcode) {
  ZeroMem (Command, 72);
  Command[0] = Opcode;
  WriteUnaligned32 ((UINT32 *)(Command + 4), Opcode == 5 ? DiskSid : NOVA_DOMAIN);
  if (Opcode != 5) {
    WriteUnaligned32 ((UINT32 *)(Command + 8), DiskSid);
  }
  Request (Opcode == 5 ? 72 : 20, Opcode == 5 ? 516 : 4);
}

STATIC VOID Reset (VIRTIO_DEVICE_PROTOCOL *Dev) {
  UINT8 Status;
  NovaCheck (Dev->SetDeviceStatus (Dev, 0), "reset");
  UINT64 Start = GetPerformanceCounter ();
  do {
    NovaCheck (Dev->GetDeviceStatus (Dev, &Status), "lecture reset");
    if (Status == 0) {
      return;
    }
  } while (GetTimeInNanoSecond (GetPerformanceCounter () - Start) < 1000000000ULL);
  NovaStop ("reset non confirmé");
}

STATIC UINT64 Config (UINTN Offset, UINTN Size) {
  UINT64 Value = 0;
  NovaCheck (Unit->ReadDevice (Unit, Offset, Size, Size, &Value), "configuration IOMMU");
  return Value;
}

EFI_STATUS NovaAttach (EFI_HANDLE Handle) {
  if (Attached) {
    if (Handle == DiskHandle) {
      return EFI_SUCCESS;
    }
    NovaLog ("NOVA DXE droit refuse hors domaine disque\r\n");
    return EFI_ACCESS_DENIED;
  }
  if (Starting) {
    return EFI_NOT_READY;
  }
  Starting = TRUE;
  EFI_STATUS S = Identify (Handle);
  if (EFI_ERROR (S)) {
    Starting = FALSE;
    return S;
  }
  if (IsMaster (DiskPci) || IsMaster (UnitPci)) {
    NovaStop ("BME déjà actif");
  }
  UINT64 Features;
  NovaCheck (Disk->GetDeviceFeatures (Disk, &Features), "capacités disque");
  if ((Features & (BIT32 | BIT33)) != (BIT32 | BIT33)) {
    NovaStop ("ACCESS_PLATFORM");
  }
  if (Config (36, 1) != 0) {
    NovaStop ("bypass au démarrage");
  }
  Reset (Unit);
  UINT8 Status = 3;
  NovaCheck (Unit->SetDeviceStatus (Unit, 1), "ACK");
  NovaCheck (Unit->SetDeviceStatus (Unit, Status), "DRIVER");
  NovaCheck (Unit->GetDeviceFeatures (Unit, &Features), "capacités IOMMU");
  CONST UINT64 Required = BIT32 | 0x57;
  if ((Features & Required) != Required) {
    NovaStop ("capacités IOMMU");
  }
  NovaCheck (Virtio10WriteFeatures (Unit, Required, &Status), "FEATURES_OK");
  if (Config (36, 1) || !(Config (0, 8) & EFI_PAGE_SIZE) || Config (32, 4) != 512 ||
      Config (8, 8) > NOVA_IOVA_BASE || Config (16, 8) < NOVA_IOVA_LIMIT - 1 ||
      Config (24, 4) > NOVA_DOMAIN || Config (28, 4) < NOVA_DOMAIN) {
    NovaStop ("géométrie IOMMU");
  }
  NovaCheck (PrivatePages (Unit, 1, (VOID **)&Command), "commande");
  NovaCheck (PrivatePages (Unit, 1, (VOID **)&Reply), "réponse");
  NovaCheck (PrivatePages (Unit, 1, (VOID **)&Fault), "faute");
  BusMaster (UnitPci, TRUE);
  Queue (0, &Requests);
  Queue (1, &Events);
  DESC_INDICES Idx;
  VirtioPrepare (&Events, &Idx);
  VirtioAppendDesc (&Events, (UINTN)Fault, 24, VRING_DESC_F_WRITE, &Idx);
  Events.Avail.Ring[0] = 0;
  MemoryFence ();
  *Events.Avail.Idx = 1;
  MemoryFence ();
  NovaCheck (Unit->SetDeviceStatus (Unit, Status | 4), "DRIVER_OK");
  NovaCheck (Unit->SetQueueNotify (Unit, 1), "file de fautes");
  EndpointCommand (5);
  if (!IsZeroBuffer (Reply, 512)) {
    NovaStop ("réserve PROBE");
  }
  NovaLog ("NOVA DXE PROBE sid=%u reserve=0 granule=4096\r\n", DiskSid);
  EndpointCommand (1);
  Attached = TRUE;
  Starting = FALSE;
  NovaLog ("NOVA DXE ATTACH sid=%u domaine=%u bypass=0 BME=0\r\n", DiskSid, NOVA_DOMAIN);
  return EFI_SUCCESS;
}

EFI_STATUS NovaRingLayout (UINTN Pages, UINTN *ReadPages, UINT16 *QueueSize) {
  UINT16 Size;
  EFI_STATUS S = Disk->GetQueueNumMax (Disk, &Size);
  if (EFI_ERROR (S) || Size < 3 || Size > 1024 || (Size & (Size - 1))) {
    return EFI_UNSUPPORTED;
  }
  // Disposition de VirtioRingInit : descripteurs/avail, puis used aligné.
  *QueueSize = Size;
  *ReadPages = EFI_SIZE_TO_PAGES (sizeof (VRING_DESC) * Size + 6 + 2 * Size);
  return Pages == *ReadPages + EFI_SIZE_TO_PAGES (6 + sizeof (VRING_USED_ELEM) * Size)
             ? EFI_SUCCESS
             : EFI_UNSUPPORTED;
}

STATIC VOID RangeCommand (UINT8 Op, UINT64 Iova, UINTN Pages) {
  ZeroMem (Command, 36);
  Command[0] = Op;
  WriteUnaligned32 ((UINT32 *)(Command + 4), NOVA_DOMAIN);
  WriteUnaligned64 ((UINT64 *)(Command + 8), Iova);
  WriteUnaligned64 ((UINT64 *)(Command + 16), Iova + EFI_PAGES_TO_SIZE (Pages) - 1);
}

VOID NovaMapRange (UINT64 Iova, UINT64 Pa, UINTN Pages, UINT32 Rights) {
  RangeCommand (3, Iova, Pages);
  WriteUnaligned64 ((UINT64 *)(Command + 24), Pa);
  WriteUnaligned32 ((UINT32 *)(Command + 32), Rights);
  Request (36, 4);
}

VOID NovaUnmapRange (UINT64 Iova, UINTN Pages) {
  RangeCommand (4, Iova, Pages);
  Request (28, 4);
}

VOID NovaQuiesce (VOID) {
  if (Attached) {
    Reset (Disk);
    NovaMaster (FALSE);
  }
}

VOID NovaDetach (VOID) {
  if (Attached) {
    EndpointCommand (2);
    Reset (Unit);
    BusMaster (UnitPci, FALSE);
    if (Config (36, 1) != 0) {
      NovaStop ("bypass après reset");
    }
    Attached = FALSE;
  }
}
