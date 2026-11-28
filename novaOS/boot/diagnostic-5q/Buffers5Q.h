/* SPDX-License-Identifier: BSD-2-Clause-Patent
   Inclus exclusivement dans le pilote XhciDxe du laboratoire. */
#include "Boite5Q.h"
STATIC VOID QStop(UINTN Reason) {
  volatile UINT64 *B = QBox();
  if (B != NULL) {
    B[10] = Reason;
    MemoryFence();
    B[2] = 6;
  }
  CpuDeadLoop();
}

typedef struct {
  UINT64 Host, Bus;
  UINTN Length;
  UINT32 Role;
} Q_GRANT;
STATIC Q_GRANT Grants[48];
STATIC UINTN Used;
STATIC Q_GRANT Bounce[2];
STATIC BOOLEAN Busy;
STATIC UINT8 *TransferHost;
STATIC UINTN TransferLength;
STATIC UINTN TransferDirection;

STATIC Q_GRANT *QGet(UINTN Role, UINTN Length) {
  volatile UINT64 *B = QBox();
  Q_GRANT *G;
  UINTN I;
  if (!B || B[2] != 0 || Used >= ARRAY_SIZE(Grants) || !Length ||
      Length > 65536) {
    QStop(1);
  }
  B[3]++;
  B[4] = Role;
  B[5] = Length;
  MemoryFence();
  B[2] = 1;
  while (B[2] == 1) {
    CpuPause();
  }
  MemoryFence();
  if (B[2] != 2 || !B[6] || !B[7] || (B[6] & 4095) || (B[7] & 4095)) {
    QStop(2);
  }
  G = &Grants[Used++];
  G->Role = (UINT32)Role;
  G->Length = Length;
  G->Host = B[6];
  G->Bus = B[7];
  for (I = 0; I + 1 < Used; I++) {
    if (G->Host < Grants[I].Host + Grants[I].Length &&
        Grants[I].Host < G->Host + Length) {
      QStop(3);
    }
  }
  B[2] = 0;
  return G;
}
STATIC VOID *QStructure(UINTN Role, UINTN Length) {
  Q_GRANT *G = QGet(Role, Length);
  return ZeroMem((VOID *)(UINTN)G->Host, Length);
}
STATIC UINTN QTranslate(UINTN Value, UINTN Size, BOOLEAN ToBus) {
  UINTN I, From, To;
  for (I = 0; I < Used; I++) {
    From = ToBus ? Grants[I].Host : Grants[I].Bus;
    To = ToBus ? Grants[I].Bus : Grants[I].Host;
    if (Size && Value >= From && Size <= Grants[I].Length &&
        Value - From <= Grants[I].Length - Size) {
      return To + (Value - From);
    }
  }
  QStop(4);
  return 0;
}
STATIC VOID QMap(URB *Urb, UINTN Dir, UINTN Len, EFI_PHYSICAL_ADDRESS *Addr,
                 VOID **Map) {
  Q_GRANT *G;
  volatile UINT64 *B = QBox();
  /* Un seul endpoint synchrone en vol sur le support admis. */
  if (!B || Busy || Dir > 1 || !Len || Len > 65536 || Urb->Data == NULL) {
    QStop(5);
  }
  G = &Bounce[Dir];
  UINTN Capacity = ALIGN_VALUE(Len, Q_PAGE);
  if (G->Length < Capacity) {
    Q_GRANT *Extra = QGet(Dir ? Q_RX : Q_TX, Capacity - G->Length);
    if (G->Length == 0) {
      *G = *Extra;
    } else {
      if (Extra->Host != G->Host + G->Length ||
          Extra->Bus != G->Bus + G->Length) {
        QStop(10);
      }
      G->Length = Capacity;
    }
  }
  if (!Dir) {
    CopyMem((VOID *)(UINTN)G->Host, Urb->Data, Len);
  }
  Busy = TRUE;
  TransferHost = Urb->Data;
  TransferLength = Len;
  TransferDirection = Dir;
  B[8]++;
  MemoryFence();
  *Addr = G->Bus;
  *Map = (VOID *)G;
}
STATIC VOID QDone(URB *Urb, EFI_STATUS Status) {
  Q_GRANT *G;
  volatile UINT64 *B = QBox();
  if (Urb->DataMap == NULL) {
    return;
  }
  G = &Bounce[TransferDirection];
  if (EFI_ERROR(Status) || Urb->Result != EFI_USB_NOERROR || !Urb->Finished ||
      !Busy || Urb->DataMap != (VOID *)G || TransferHost != Urb->Data ||
      Urb->Completed > TransferLength) {
    QStop(6);
  }
  MemoryFence();
  if (TransferDirection) {
    CopyMem(TransferHost, (VOID *)(UINTN)G->Host, Urb->Completed);
  }
  Urb->DataMap = NULL;
  Busy = FALSE;
  B[9]++;
}
