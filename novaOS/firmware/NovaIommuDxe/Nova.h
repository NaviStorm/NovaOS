// SPDX-License-Identifier: MIT
#include <Uefi.h>
#include <Protocol/FdtClient.h>
#include <Protocol/IoMmu.h>
#include <Protocol/PciIo.h>
#include <Protocol/VirtioDevice.h>
#include <Library/BaseLib.h>
#include <Library/BaseMemoryLib.h>
#include <Library/PrintLib.h>
#include <Library/SerialPortLib.h>
#include <Library/TimerLib.h>
#include <Library/UefiBootServicesTableLib.h>
#include <Library/VirtioLib.h>

#define NOVA_DOMAIN 2
#define NOVA_IOVA_BASE 0x20000000U
#define NOVA_IOVA_LIMIT 0xf0000000U

VOID NovaLog (CONST CHAR8 *Format, ...);
VOID NovaStop (CONST CHAR8 *Reason);
VOID NovaCheck (EFI_STATUS Status, CONST CHAR8 *Step);
EFI_STATUS NovaTopology (VOID);
EFI_STATUS NovaAttach (EFI_HANDLE Handle);
EFI_STATUS NovaRingLayout (UINTN Pages, UINTN *ReadPages, UINT16 *QueueSize);
VOID NovaMaster (BOOLEAN Enable);
VOID NovaMapRange (UINT64 Iova, UINT64 Pa, UINTN Pages, UINT32 Rights);
VOID NovaUnmapRange (UINT64 Iova, UINTN Pages);
VOID NovaQuiesce (VOID);
VOID NovaDetach (VOID);
