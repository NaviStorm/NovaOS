/* SPDX-License-Identifier: BSD-2-Clause-Patent
   Contrat privé du firmware Q35. La boîte CPU est exclue des domaines DMA. */
#include <Library/BaseLib.h>
#include <Library/IoLib.h>
#include <Library/QemuFwCfgLib.h>

#define Q_MAGIC 0x3542514f564f4eULL
#define Q_PAGE 4096U
enum { Q_DCBAA = 1, Q_EVENT, Q_ERST, Q_RING, Q_INPUT, Q_OUTPUT, Q_RX, Q_TX };
STATIC volatile UINT64 *QBox(VOID) {
  STATIC volatile UINT64 *Cached;
  FIRMWARE_CONFIG_ITEM Item;
  UINTN Size;
  UINT64 Address;
  if (Cached != NULL) {
    return Cached;
  }
  if (EFI_ERROR(QemuFwCfgFindFile("opt/novaos/5q-locator", &Item, &Size)) ||
      Size != 8) {
    return NULL;
  }
  QemuFwCfgSelectItem(Item);
  Address = QemuFwCfgRead64();
  if (Address < BASE_4GB || (Address & 4095) != 0) {
    return NULL;
  }
  Cached = (volatile UINT64 *)(UINTN)Address;
  if (Cached[0] != Q_MAGIC) {
    Cached = NULL;
  }
  return Cached;
}
