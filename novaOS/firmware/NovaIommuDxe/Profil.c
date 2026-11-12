// SPDX-License-Identifier: MIT
// Profil virt-10.0. QEMU résout pcidev vers un BDF en construisant le type SMBIOS 41.
#include "Nova.h"
#include <Protocol/Smbios.h>
#include <Protocol/Tcg2Protocol.h>

EFI_STATUS NovaRoles (UINT32 *Source, UINT32 *Target, BOOLEAN *System) {
  EFI_SMBIOS_PROTOCOL *Smbios;
  EFI_STATUS S = gBS->LocateProtocol (&gEfiSmbiosProtocolGuid, NULL, (VOID **)&Smbios);
  if (EFI_ERROR (S)) {
    return S;
  }
  EFI_SMBIOS_HANDLE Handle = SMBIOS_HANDLE_PI_RESERVED;
  EFI_SMBIOS_TYPE Type = SMBIOS_TYPE_ONBOARD_DEVICES_EXTENDED_INFORMATION;
  EFI_SMBIOS_TABLE_HEADER *Record;
  *System = FALSE;
  *Source = *Target = MAX_UINT32;
  while ((S = Smbios->GetNext (Smbios, &Handle, &Type, &Record, NULL)) == EFI_SUCCESS) {
    if (Record->Length != sizeof (SMBIOS_TABLE_TYPE41)) {
      return EFI_UNSUPPORTED;
    }
    SMBIOS_TABLE_TYPE41 *Port = (SMBIOS_TABLE_TYPE41 *)Record;
    // Le générateur QEMU place la désignation dans l'unique chaîne, numéro 1.
    if (Port->ReferenceDesignation != 1) {
      return EFI_UNSUPPORTED;
    }
    CONST CHAR8 *Name = (CONST CHAR8 *)Record + Record->Length;
    UINT32 *Role;
    if (AsciiStrCmp (Name, "NovaOS-Install-v1") == 0) {
      Role = Source;
    } else if (AsciiStrCmp (Name, "NovaOS-System-v1") == 0) {
      *System = TRUE;
      Role = Source;
    } else if (AsciiStrCmp (Name, "NovaOS-Target-v1") == 0) {
      Role = Target;
    } else {
      continue;
    }
    if (*Role != MAX_UINT32 || Port->DeviceType != 0x81 || Port->DeviceTypeInstance != 1 ||
        Port->SegmentGroupNum != 0 || Port->BusNum != 0 || (Port->DevFuncNum & 7) != 0) {
      return EFI_ACCESS_DENIED;
    }
    *Role = Port->DevFuncNum;
  }
  if (S != EFI_NOT_FOUND || *Source == MAX_UINT32 || *Source == *Target || (*System && *Target != MAX_UINT32)) {
    return EFI_ACCESS_DENIED;
  }
  return EFI_SUCCESS;
}

// TPM2_GetCapability(TPM_CAP_TPM_PROPERTIES, TPM_PT_MANUFACTURER, 1).
// La structure TPM sur le fil est big-endian. Aucun résultat logiciel de secours.
EFI_STATUS NovaCheckTpm (VOID) {
  EFI_TCG2_PROTOCOL *Tcg;
  EFI_TCG2_BOOT_SERVICE_CAPABILITY Cap = {0};
  UINT8 Cmd[] = {0x80,1, 0,0,0,22, 0,0,1,0x7a, 0,0,0,6, 0,0,1,5, 0,0,0,1};
  UINT8 Response[27] = {0};
  EFI_STATUS S = gBS->LocateProtocol (&gEfiTcg2ProtocolGuid, NULL, (VOID **)&Tcg);
  if (EFI_ERROR (S)) {
    return S;
  }
  Cap.Size = sizeof Cap;
  S = Tcg->GetCapability (Tcg, &Cap);
  if (EFI_ERROR (S) || !Cap.TPMPresentFlag) {
    return EFI_DEVICE_ERROR;
  }
  S = Tcg->SubmitCommand (Tcg, sizeof Cmd, Cmd, sizeof Response, Response);
  if (EFI_ERROR (S) || Response[0] != 0x80 || Response[1] != 1 ||
      SwapBytes32 (ReadUnaligned32 ((UINT32 *)(Response + 2))) != sizeof Response ||
      ReadUnaligned32 ((UINT32 *)(Response + 6)) != 0 || Response[10] > 1 ||
      SwapBytes32 (ReadUnaligned32 ((UINT32 *)(Response + 11))) != 6 ||
      SwapBytes32 (ReadUnaligned32 ((UINT32 *)(Response + 15))) != 1 ||
      SwapBytes32 (ReadUnaligned32 ((UINT32 *)(Response + 19))) != 0x105) {
    return EFI_DEVICE_ERROR;
  }
  UINT32 Vendor = SwapBytes32 (ReadUnaligned32 ((UINT32 *)(Response + 23)));
  if (Vendor == 0 || Vendor == MAX_UINT32) {
    return EFI_DEVICE_ERROR;
  }
  NovaLog ("NOVA TPM GetCapability fabricant=0x%x\r\n", Vendor);
  return EFI_SUCCESS;
}
