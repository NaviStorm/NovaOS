// SPDX-License-Identifier: MIT
// Les métadonnées du fournisseur et les files de l’IOMMU restent privées.
#include "Nova.h"
#include <Guid/EventGroup.h>

typedef struct {
  VOID *Pointer;
  UINTN Pages;
} COMMON;

typedef struct {
  UINTN Token;
  EFI_HANDLE Owner;
  VOID *Host;
  VOID *Buffer;
  UINT64 Iova;
  UINTN Bytes;
  UINTN Pages;
  UINTN ReadPages;
  UINT32 Rights;
  UINT16 QueueSize;
  BOOLEAN Common, Active, Completed;
} MAPPING;

STATIC COMMON Buffers[32];
STATIC MAPPING Maps[32];
STATIC MAPPING *DiskRing;
STATIC UINT64 NextIova = NOVA_IOVA_BASE;
STATIC UINTN NextToken, Granted, Revoked;
STATIC BOOLEAN Closed, MasterOn;
STATIC EFI_EVENT BeforeExit;

STATIC MAPPING *Find (VOID *Token) {
  if (Token != NULL) {
    for (UINTN I = 0; I < ARRAY_SIZE (Maps); ++I) {
      if (Maps[I].Token == (UINTN)Token) {
        return &Maps[I];
      }
    }
  }
  return NULL;
}

STATIC COMMON *FindCommon (VOID *Host) {
  if (Host != NULL) {
    for (UINTN I = 0; I < ARRAY_SIZE (Buffers); ++I) {
      if (Buffers[I].Pointer == Host) {
        return &Buffers[I];
      }
    }
  }
  return NULL;
}

STATIC EFI_STATUS EFIAPI Allocate (EDKII_IOMMU_PROTOCOL *This, EFI_ALLOCATE_TYPE Type,
                                   EFI_MEMORY_TYPE MemoryType, UINTN Pages, VOID **Host,
                                   UINT64 Attributes) {
  if (Closed || Host == NULL || MemoryType != EfiBootServicesData || Pages == 0 ||
      Pages > (NOVA_IOVA_LIMIT - NOVA_IOVA_BASE) / EFI_PAGE_SIZE ||
      (Attributes & EDKII_IOMMU_ATTRIBUTE_INVALID_FOR_ALLOCATE_BUFFER)) {
    return EFI_UNSUPPORTED;
  }
  EFI_TPL Previous = gBS->RaiseTPL (TPL_NOTIFY);
  EFI_STATUS S = EFI_OUT_OF_RESOURCES;
  for (UINTN I = 0; I < ARRAY_SIZE (Buffers); ++I) {
    if (Buffers[I].Pointer != NULL) {
      continue;
    }
    // Les périphériques voient exclusivement des IOVA 32 bits. Type est
    // ignoré comme prescrit pour AllocateBuffer du pont racine PCI.
    EFI_PHYSICAL_ADDRESS Pa = 0;
    S = gBS->AllocatePages (AllocateAnyPages, MemoryType, Pages, &Pa);
    if (!EFI_ERROR (S)) {
      *Host = (VOID *)(UINTN)Pa;
      ZeroMem (*Host, EFI_PAGES_TO_SIZE (Pages));
      Buffers[I] = (COMMON){*Host, Pages};
    }
    break;
  }
  gBS->RestoreTPL (Previous);
  return S;
}

STATIC EFI_STATUS EFIAPI Free (EDKII_IOMMU_PROTOCOL *This, UINTN Pages, VOID *Host) {
  EFI_TPL Previous = gBS->RaiseTPL (TPL_NOTIFY);
  EFI_STATUS S = EFI_ACCESS_DENIED;
  COMMON *B = FindCommon (Host);
  if (!Closed && B != NULL && Pages == B->Pages) {
    for (UINTN I = 0; I < ARRAY_SIZE (Maps); ++I) {
      MAPPING *M = &Maps[I];
      // Un tampon de transit peut encore devoir recopier vers une sous-plage
      // de cette allocation après UNMAP. Retenir aussi sa destination CPU.
      if (M->Token && ((UINTN)M->Host < (UINTN)Host + EFI_PAGES_TO_SIZE (Pages)) &&
          (UINTN)Host < (UINTN)M->Host + M->Bytes) {
        goto Done;
      }
    }
    ZeroMem (Host, EFI_PAGES_TO_SIZE (Pages));
    S = gBS->FreePages ((UINTN)Host, Pages);
    if (!EFI_ERROR (S)) {
      ZeroMem (B, sizeof *B);
    }
  }
Done:
  gBS->RestoreTPL (Previous);
  return S;
}

STATIC EFI_STATUS EFIAPI Map (EDKII_IOMMU_PROTOCOL *This, EDKII_IOMMU_OPERATION Op, VOID *Host,
                              UINTN *Bytes, EFI_PHYSICAL_ADDRESS *Device, VOID **Token) {
  if (Closed || Host == NULL || Bytes == NULL || Device == NULL || Token == NULL ||
      (UINT32)Op >= EdkiiIoMmuOperationMaximum || *Bytes == 0 ||
      *Bytes > NOVA_IOVA_LIMIT - NOVA_IOVA_BASE || (UINTN)Host > MAX_UINTN - *Bytes) {
    return EFI_INVALID_PARAMETER;
  }
  EFI_TPL Previous = gBS->RaiseTPL (TPL_NOTIFY);
  EFI_STATUS S = EFI_OUT_OF_RESOURCES;
  UINTN Pages = EFI_SIZE_TO_PAGES (*Bytes);
  UINT32 Rights = (UINT32)Op % 3 + 1;
  VOID *Buffer = Host;
  if (EFI_PAGES_TO_SIZE (Pages) > NOVA_IOVA_LIMIT - NextIova) {
    goto Done;
  }
  for (UINTN I = 0; I < ARRAY_SIZE (Maps); ++I) {
    MAPPING *M = &Maps[I];
    if (M->Token) {
      continue;
    }
    if (Rights == 3) {
      COMMON *B = FindCommon (Host);
      if (B == NULL || B->Pages != Pages) {
        S = EFI_ACCESS_DENIED;
        break;
      }
      for (UINTN J = 0; J < ARRAY_SIZE (Maps); ++J) {
        if (Maps[J].Token && Maps[J].Buffer == Host) {
          S = EFI_ACCESS_DENIED;
          goto Done;
        }
      }
    } else {
      EFI_PHYSICAL_ADDRESS Pa = 0;
      S = gBS->AllocatePages (AllocateAnyPages, EfiBootServicesData, Pages, &Pa);
      if (EFI_ERROR (S)) {
        break;
      }
      Buffer = (VOID *)(UINTN)Pa;
      ZeroMem (Buffer, EFI_PAGES_TO_SIZE (Pages));
      if (Rights == EDKII_IOMMU_ACCESS_READ) {
        CopyMem (Buffer, Host, *Bytes);
      }
    }
    *M = (MAPPING){.Token = ++NextToken,
                   .Host = Host,
                   .Buffer = Buffer,
                   .Iova = NextIova,
                   .Bytes = *Bytes,
                   .Pages = Pages,
                   .Rights = Rights,
                   .Common = Rights == 3};
    *Device = NextIova;
    *Token = (VOID *)M->Token;
    NextIova += EFI_PAGES_TO_SIZE (Pages);
    S = EFI_SUCCESS;
    break;
  }
Done:
  gBS->RestoreTPL (Previous);
  return S;
}

STATIC VOID VerifyIdle (VOID) {
  MAPPING *R = DiskRing;
  if (R == NULL) {
    NovaStop ("anneau absent");
  }
  volatile UINT8 *Base = R->Buffer;
  volatile UINT16 *Avail = (volatile UINT16 *)(Base + sizeof (VRING_DESC) * R->QueueSize);
  volatile UINT16 *Used = (volatile UINT16 *)(Base + EFI_PAGES_TO_SIZE (R->ReadPages));
  UINT16 End = Avail[1];
  MemoryFence ();
  if (Used[1] != End) {
    NovaStop ("transaction non terminee");
  }
  MemoryFence ();
  UINTN Last = (UINT16)(End - 1) % R->QueueSize;
  volatile VRING_USED_ELEM *Elem = (volatile VRING_USED_ELEM *)(Used + 2);
  if (Elem[Last].Id != Avail[2 + Last] || Elem[Last].Id != 0) {
    NovaStop ("descripteur utilise");
  }
}

STATIC VOID Revoke (MAPPING *M) {
  VerifyIdle ();
  if (M == DiskRing) {
    for (UINTN I = 0; I < ARRAY_SIZE (Maps); ++I) {
      if (&Maps[I] != M && Maps[I].Active) {
        NovaStop ("retrait anneau avec tampons actifs");
      }
    }
    NovaQuiesce (Closed);
    MasterOn = FALSE;
    DiskRing = NULL;
  }
  if (M->ReadPages != 0) {
    NovaUnmapRange (M->Iova, M->ReadPages);
  }
  NovaUnmapRange (M->Iova + EFI_PAGES_TO_SIZE (M->ReadPages), M->Pages - M->ReadPages);
  M->Active = FALSE;
  M->Completed = TRUE;
  ++Revoked;
}

STATIC EFI_STATUS EFIAPI SetAttribute (EDKII_IOMMU_PROTOCOL *This, EFI_HANDLE Handle, VOID *Token,
                                       UINT64 Access) {
  if (Closed || Handle == NULL || Find (Token) == NULL) {
    return EFI_ACCESS_DENIED;
  }
  EFI_STATUS S = NovaAttach (Handle);
  if (EFI_ERROR (S)) {
    return S;
  }
  EFI_TPL Previous = gBS->RaiseTPL (TPL_NOTIFY);
  MAPPING *M = Find (Token);
  S = EFI_ACCESS_DENIED;
  if (M == NULL || Closed || (M->Owner != NULL && M->Owner != Handle)) {
    goto Done;
  }
  if (Access == 0) {
    if (M->Active) {
      Revoke (M);
    }
    S = EFI_SUCCESS;
    goto Done;
  }
  if (Access != M->Rights || M->Active || M->Completed) {
    goto Done;
  }
  UINT32 Rights = M->Rights;
  if (DiskRing == NULL && (!M->Common || M->Bytes == 1)) {
    goto Done;
  }
  if (M->Common) {
    if (M->Bytes == 1) {
      Rights = EDKII_IOMMU_ACCESS_WRITE; // Page dédiée au seul octet de statut.
    } else {
      if (DiskRing != NULL || M->Bytes != EFI_PAGES_TO_SIZE (M->Pages) ||
          EFI_ERROR (NovaRingLayout (M->Pages, &M->ReadPages, &M->QueueSize))) {
        goto Done;
      }
      DiskRing = M;
      NovaMapRange (M->Iova, (UINTN)M->Buffer, M->ReadPages, EDKII_IOMMU_ACCESS_READ);
    }
  }
  UINTN Offset = EFI_PAGES_TO_SIZE (M->ReadPages);
  NovaMapRange (M->Iova + Offset, (UINTN)M->Buffer + Offset, M->Pages - M->ReadPages, Rights);
  M->Owner = Handle;
  M->Active = TRUE;
  ++Granted;
  if (!MasterOn) {
    NovaMaster (TRUE);
    MasterOn = TRUE;
    NovaLog ("NOVA DXE premier MAP acquitte, BME=1\r\n");
  }
  S = EFI_SUCCESS;
Done:
  gBS->RestoreTPL (Previous);
  return S;
}

STATIC EFI_STATUS EFIAPI Unmap (EDKII_IOMMU_PROTOCOL *This, VOID *Token) {
  EFI_TPL Previous = gBS->RaiseTPL (TPL_NOTIFY);
  MAPPING *M = Find (Token);
  EFI_STATUS S = EFI_ACCESS_DENIED;
  if (!Closed && M != NULL && !M->Active) {
    if (!M->Common) {
      if (M->Completed && M->Rights == EDKII_IOMMU_ACCESS_WRITE) {
        CopyMem (M->Host, M->Buffer, M->Bytes);
      }
      ZeroMem (M->Buffer, EFI_PAGES_TO_SIZE (M->Pages));
      NovaCheck (gBS->FreePages ((UINTN)M->Buffer, M->Pages), "libération après retrait");
    }
    ZeroMem (M, sizeof *M);
    S = EFI_SUCCESS;
  }
  gBS->RestoreTPL (Previous);
  return S;
}

STATIC VOID EFIAPI Close (EFI_EVENT Event, VOID *Context) {
  if (Closed) {
    return;
  }
  Closed = TRUE;
  for (UINTN I = 0; I < ARRAY_SIZE (Maps); ++I) {
    MAPPING *M = &Maps[I];
    if (!M->Token) {
      continue;
    }
    // À la sortie seul l’anneau du disque au repos peut rester publié.
    if (!M->Active || M != DiskRing) {
      NovaStop ("transaction en cours à la sortie");
    }
  }
  if (DiskRing != NULL) {
    MAPPING *M = DiskRing;
    Revoke (M);
    ZeroMem (M->Buffer, EFI_PAGES_TO_SIZE (M->Pages));
    ZeroMem (M, sizeof *M);
  }
  NovaDetach ();
  if (Granted == 0 || Granted != Revoked) {
    NovaStop ("bilan des droits");
  }
  NovaLog ("NOVA DXE SORTIE droits=%u retraits=%u actifs=0 BME=0 bypass=0\r\n", Granted, Revoked);
}

STATIC EDKII_IOMMU_PROTOCOL Protocol = {
    EDKII_IOMMU_PROTOCOL_REVISION, SetAttribute, Map, Unmap, Allocate, Free};

EFI_STATUS EFIAPI NovaEntry (EFI_HANDLE Image, EFI_SYSTEM_TABLE *System) {
  EFI_STATUS S = NovaTopology ();
  if (EFI_ERROR (S)) {
    NovaLog ("NOVA DXE topologie refusee %r\r\n", S);
    return S;
  }
  S = gBS->CreateEventEx (EVT_NOTIFY_SIGNAL, TPL_NOTIFY, Close, NULL,
                          &gEfiEventBeforeExitBootServicesGuid, &BeforeExit);
  if (EFI_ERROR (S)) {
    return S;
  }
  S = gBS->InstallProtocolInterface (&Image, &gEdkiiIoMmuProtocolGuid, EFI_NATIVE_INTERFACE,
                                     &Protocol);
  if (EFI_ERROR (S)) {
    gBS->CloseEvent (BeforeExit);
    return S;
  }
  NovaLog ("NOVA DXE politique active avant PCI\r\n");
  return EFI_SUCCESS;
}
