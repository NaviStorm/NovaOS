// Témoin EFI du banc Q35 exclusivement. Sortie sur le port de diagnostic QEMU.
#include <Uefi.h>
#include <Guid/GlobalVariable.h>
#include <Protocol/DevicePath.h>
#include <Protocol/LoadedImage.h>

void ligne(const char *texte) {
  while (*texte) {
    __asm__ volatile ("outb %0, %1" : : "a"(*texte++), "Nd"((UINT16)0xe9));
  }
}

void nombre(UINT64 valeur) {
  char chiffre[2] = {0, 0};
  for (int i = 60; i >= 0; i -= 4) {
    chiffre[0] = "0123456789abcdef"[(valeur >> i) & 15];
    ligne(chiffre);
  }
}

static BOOLEAN variable(EFI_SYSTEM_TABLE *st, CHAR16 *nom, UINT8 attendue) {
  EFI_GUID global = EFI_GLOBAL_VARIABLE;
  UINT8 valeur = 0xff;
  UINTN taille = sizeof(valeur);
  return st->RuntimeServices->GetVariable(nom, &global, NULL, &taille, &valeur) == EFI_SUCCESS &&
         taille == sizeof(valeur) && valeur == attendue;
}

// Le chemin absolu conserve le média de l'image exécutée. Le firmware ouvre le
// fichier lui-même pour appliquer sa politique de vérification de ce média.
static EFI_STATUS charger(EFI_SYSTEM_TABLE *st, EFI_HANDLE image, const CHAR16 *nom, UINTN octets, BOOLEAN *ouverte) {
  EFI_GUID chargee = EFI_LOADED_IMAGE_PROTOCOL_GUID;
  EFI_GUID chemin = EFI_DEVICE_PATH_PROTOCOL_GUID;
  EFI_LOADED_IMAGE_PROTOCOL *origine = NULL;
  EFI_DEVICE_PATH_PROTOCOL *racine = NULL;
  EFI_BOOT_SERVICES *bs = st->BootServices;
  EFI_STATUS statut = bs->HandleProtocol(image, &chargee, (VOID **)&origine);
  if (EFI_ERROR(statut)) return statut;
  statut = bs->HandleProtocol(origine->DeviceHandle, &chemin, (VOID **)&racine);
  if (EFI_ERROR(statut)) return statut;

  UINT8 buf[1024];
  UINTN n = 0;
  for (;;) {
    UINTN len = racine->Length[0] | ((UINTN)racine->Length[1] << 8);
    if (len < sizeof(*racine) || len > sizeof(buf) - n) return EFI_BAD_BUFFER_SIZE;
    if (racine->Type == END_DEVICE_PATH_TYPE) {
      if (racine->SubType != END_ENTIRE_DEVICE_PATH_SUBTYPE || len != sizeof(*racine))
        return EFI_UNSUPPORTED;
      break;
    }
    bs->CopyMem(buf + n, racine, len);
    racine = (VOID *)((UINT8 *)racine + len);
    n += len;
  }

  UINTN fichier = sizeof(EFI_DEVICE_PATH_PROTOCOL) + octets;
  if (fichier + sizeof(*racine) > sizeof(buf) - n) return EFI_BAD_BUFFER_SIZE;
  EFI_DEVICE_PATH_PROTOCOL entete = { MEDIA_DEVICE_PATH, MEDIA_FILEPATH_DP,
                                      {(UINT8)fichier, (UINT8)(fichier >> 8)} };
  bs->CopyMem(buf + n, &entete, sizeof(entete));
  bs->CopyMem(buf + n + sizeof(entete), (VOID *)nom, octets);
  bs->CopyMem(buf + n + fichier, racine, sizeof(*racine));
  EFI_HANDLE enfant = NULL;
  statut = bs->LoadImage(FALSE, image, (EFI_DEVICE_PATH_PROTOCOL *)buf, NULL, 0, &enfant);
  *ouverte = enfant != NULL;
  if (enfant != NULL && bs->UnloadImage(enfant) != EFI_SUCCESS) return EFI_DEVICE_ERROR;
  return statut;
}

// Une seule mesure, mêmes réservations pour les deux firmwares. Le tampon
// dimensionné du témoin est lui-même compté dans les pages LoaderData.
static BOOLEAN mesurer(EFI_BOOT_SERVICES *bs) {
  static UINT64 carte[4096];
  UINTN len = sizeof(carte), cle = 0, pas = 0;
  UINT32 version = 0;
  UINT64 boot = 0, runtime = 0, libre = 0;
  if (bs->GetMemoryMap(&len, (VOID *)carte, &cle, &pas, &version) != EFI_SUCCESS ||
      pas < sizeof(EFI_MEMORY_DESCRIPTOR) || len % pas != 0 ||
      version != EFI_MEMORY_DESCRIPTOR_VERSION) return FALSE;
  for (UINTN i = 0; i < len; i += pas) {
    EFI_MEMORY_DESCRIPTOR *d = (VOID *)((UINT8 *)carte + i);
    if (d->Type == EfiBootServicesCode || d->Type == EfiBootServicesData) boot += d->NumberOfPages;
    if (d->Type == EfiRuntimeServicesCode || d->Type == EfiRuntimeServicesData) runtime += d->NumberOfPages;
    if (d->Type == EfiConventionalMemory) libre += d->NumberOfPages;
  }
  ligne("NET EFI pages boot="); nombre(boot);
  ligne(" runtime="); nombre(runtime);
  ligne(" libres="); nombre(libre); ligne("\n");
  return TRUE;
}

EFI_STATUS EFIAPI EfiMain(EFI_HANDLE image, EFI_SYSTEM_TABLE *st) {
  if (!variable(st, L"SecureBoot", 1) || !variable(st, L"SetupMode", 0)) {
    ligne("NET EFI politique inattendue\n");
    return EFI_SECURITY_VIOLATION;
  }
  ligne("NET EFI SecureBoot=1 SetupMode=0\n");
  const CHAR16 admise[] = L"\\EFI\\BOOT\\BOOTX64.EFI";
  const CHAR16 refusee[] = L"\\EFI\\BOOT\\REFUSE.EFI";
  BOOLEAN ouverte = FALSE;
  EFI_STATUS statut = charger(st, image, admise, sizeof(admise), &ouverte);
  ligne("NET EFI LoadImage admise="); nombre(statut); ligne("\n");
  if (statut != EFI_SUCCESS || !ouverte) return EFI_ABORTED;
  ouverte = TRUE;
  statut = charger(st, image, refusee, sizeof(refusee), &ouverte);
  ligne("NET EFI LoadImage refusee="); nombre(statut); ligne("\n");
  if (statut != EFI_ACCESS_DENIED || ouverte) return EFI_ABORTED;
  if (!mesurer(st->BootServices)) return EFI_ABORTED;
  ligne("NET EFI controle fini\n");
#ifdef NOVA_VTD_TERMINAL
  extern void vtd_terminal(EFI_HANDLE, EFI_SYSTEM_TABLE *);
  vtd_terminal(image, st);
#endif
  // Garder l'instant du contrôle disponible pour les lectures QMP du banc.
  for (;;) __asm__ volatile ("hlt");
}
