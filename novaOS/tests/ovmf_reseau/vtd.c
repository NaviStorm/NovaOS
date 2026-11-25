// Point terminal de qualification Q35 TCG. Aucun domaine ne reçoit de traduction.
// Le témoin ne rend jamais la main à un OS après ExitBootServices.
#include <Uefi.h>
#include <Protocol/AcpiSystemDescriptionTable.h>
#include <Protocol/PciIo.h>

void ligne(const char *);
void nombre(UINT64);

#define DRHD 0xfed90000UL
#define REG32(off) (*(volatile UINT32 *)(UINTN)(DRHD + (off)))
#define REG64(off) (*(volatile UINT64 *)(UINTN)(DRHD + (off)))
#define ROOT_OK (1U << 30)
#define QI_ON   (1U << 26)
#define TES     (1U << 31)

typedef struct { UINT8 sid; UINT32 id; } Fonction;
static const Fonction profil[] = {
  {0, 0x29c08086}, {56, 0x10411af4}, {248, 0x29188086}, {250, 0x29228086}, {251, 0x29308086}
};

static void fin(const char *phase) {
  ligne("NET VTD terminal "); ligne(phase); ligne("\n");
  for (;;) __asm__ volatile ("cli; hlt");
}

static UINT32 cfg(UINT8 sid, UINT8 offset) {
  UINT32 selecteur = 0x80000000U | ((UINT32)sid << 8) | offset;
  UINT32 val;
  __asm__ volatile ("outl %0,%w1" : : "a"(selecteur), "Nd"((UINT16)0xcf8));
  __asm__ volatile ("inl %w1,%0" : "=a"(val) : "Nd"((UINT16)0xcfc));
  return val;
}

static BOOLEAN topologie(BOOLEAN fermee) {
  UINTN n = 0;
  for (UINTN sid = 0; sid < 256; ++sid) {
    UINT32 id = cfg((UINT8)sid, 0);
    if (id == 0xffffffffU) continue;
    if (n >= sizeof(profil) / sizeof(profil[0]) || sid != profil[n].sid || id != profil[n].id)
      return FALSE;
    if ((fermee || sid == 56) && (cfg((UINT8)sid, 4) & 4)) return FALSE;
    ++n;
  }
  return n == sizeof(profil) / sizeof(profil[0]);
}

static BOOLEAN dmar(EFI_BOOT_SERVICES *bs) {
  EFI_GUID guid = EFI_ACPI_SDT_PROTOCOL_GUID;
  EFI_ACPI_SDT_PROTOCOL *sdt = NULL;
  if (bs->LocateProtocol(&guid, NULL, (VOID **)&sdt) != EFI_SUCCESS) return FALSE;
  BOOLEAN found = FALSE;
  for (UINTN i = 0; i < 128; ++i) {
    EFI_ACPI_SDT_HEADER *h = NULL;
    EFI_ACPI_TABLE_VERSION ver;
    UINTN key;
    EFI_STATUS s = sdt->GetAcpiTable(i, &h, &ver, &key);
    if (s == EFI_NOT_FOUND) return found;
    if (s != EFI_SUCCESS || h == NULL) return FALSE;
    if (h->Signature != SIGNATURE_32('D','M','A','R')) continue;
    const UINT8 *p = (const UINT8 *)h;
    if (found || h->Revision != 1 || h->Length < 64 || h->Length > 128 ||
        (h->Length - 64) % 8 || p[36] != 47 || (p[37] & ~1)) return FALSE;
    UINT8 sum = 0;
    for (UINTN k = 0; k < h->Length; ++k) sum += p[k];
    for (UINTN k = 38; k < 48; ++k) if (p[k]) return FALSE;
    if (sum || p[48] || p[49] || p[50] != h->Length - 48 || p[51] ||
        p[52] > 1 || p[53] || p[54] || p[55]) return FALSE;
    UINT64 base = 0;
    bs->CopyMem(&base, (VOID *)(p+56), sizeof(base));
    if (base != DRHD) return FALSE;
    UINT32 vus = 0;
    BOOLEAN ioapic = FALSE;
    for (UINTN k = 64; k < h->Length; k += 8) {
      if (p[k+1] != 8 || p[k+2] || p[k+3] || p[k+4]) return FALSE;
      if (p[k] == 3 && p[k+5] == 255 && !p[k+6] && !p[k+7] && !ioapic) {
        ioapic = TRUE;
        continue;
      }
      if (p[52] || p[k] != 1 || p[k+5] || p[k+6] >= 32 || p[k+7] >= 8) return FALSE;
      UINTN n = 0;
      while (n < 5 && (p[k+6]*8 + p[k+7]) != profil[n].sid) ++n;
      if (n == 5 || (vus & (1U << n))) return FALSE;
      vus |= 1U << n;
    }
    if (!ioapic || (!p[52] && vus != 31)) return FALSE;
    found = TRUE;
  }
  return FALSE;
}

static BOOLEAN fermer_pairs(EFI_BOOT_SERVICES *bs) {
  EFI_GUID guid = EFI_PCI_IO_PROTOCOL_GUID;
  EFI_HANDLE *handles = NULL;
  UINTN count = 0;
  if (bs->LocateHandleBuffer(ByProtocol, &guid, NULL, &count, &handles) != EFI_SUCCESS)
    return FALSE;
  if (count != sizeof(profil) / sizeof(profil[0])) {
    bs->FreePool(handles);
    return FALSE;
  }
  UINT32 vus = 0;
  for (UINTN i = 0; i < count; ++i) {
    EFI_PCI_IO_PROTOCOL *io = NULL;
    UINTN seg, bus, dev, fun;
    if (bs->HandleProtocol(handles[i], &guid, (VOID **)&io) != EFI_SUCCESS ||
        io->GetLocation(io, &seg, &bus, &dev, &fun) != EFI_SUCCESS ||
        seg || bus || dev > 31 || fun > 7) break;
    UINTN n = 0;
    while (n < 5 && profil[n].sid != dev*8+fun) ++n;
    if (n == 5 || (vus & (1U << n))) break;
    if (profil[n].sid == 250) {
      // Fin de toute E/S firmware via son pilote SATA et ses enfants synthétiques.
      if (bs->DisconnectController(handles[i], NULL, NULL) != EFI_SUCCESS) break;
    }
    if (profil[n].sid != 56 && (cfg(profil[n].sid, 4) & 4) &&
        io->Attributes(io, EfiPciIoAttributeOperationDisable,
                       EFI_PCI_IO_ATTRIBUTE_BUS_MASTER, NULL) != EFI_SUCCESS) break;
    vus |= 1U << n;
  }
  bs->FreePool(handles);
  return vus == 31 && topologie(TRUE);
}

static BOOLEAN attendre32(UINTN offset, UINT32 attendu) {
  for (UINTN i = 0; i < 1000000; ++i) {
    if (REG32(offset) == attendu) return TRUE;
    __asm__ volatile ("pause");
  }
  return FALSE;
}

// Aucune écriture à l'unité avant d'avoir quitté les services du firmware.
// Les trois pages réservées (root vide, QI, reçu) persistent jusqu'à l'arrêt de la VM.
void vtd_terminal(EFI_HANDLE image, EFI_SYSTEM_TABLE *st) {
  EFI_BOOT_SERVICES *bs = st->BootServices;
  UINT32 a = 0x40000000, b, c, d;
  __asm__ volatile ("cpuid" : "+a"(a), "=b"(b), "=c"(c), "=d"(d));
  if (b != SIGNATURE_32('T','C','G','T') || c != SIGNATURE_32('C','G','T','C') ||
      d != SIGNATURE_32('G','T','C','G')) fin("profil-tcg-refuse");
  if (!topologie(FALSE)) fin("topologie-refusee");
  if (!dmar(bs)) fin("dmar-refusee");
  UINT64 cap = REG64(8), ecap = REG64(16);
  if (REG32(0) != 0x10 || REG32(0x1c) != 0 || REG64(0x20) != 0 ||
      REG32(0x34) || REG64(0x80) || REG64(0x88) || REG64(0x90) ||
      ((cap >> 16) & 63) != 47 || (cap & (3ULL << 54)) != (3ULL << 54) ||
      (cap & (1ULL << 4)) || !(ecap & 2)) fin("etat-initial-refuse");
  a = 1;
  __asm__ volatile ("cpuid" : "+a"(a), "=b"(b), "=c"(c), "=d"(d));
  UINTN line = ((b >> 8) & 255)*8;
  if (!(d & (1U<<19)) || !line || line > 4096 || (line & (line-1))) fin("coherence-refusee");
  EFI_PHYSICAL_ADDRESS racine = 0;
  if (bs->AllocatePages(AllocateAnyPages, EfiLoaderData, 3, &racine) != EFI_SUCCESS ||
      !racine || (racine & 4095) || racine >= (1ULL << 48)-12288) fin("allocation-refusee");
  bs->SetMem((VOID *)(UINTN)racine, 12288, 0);
  volatile UINT64 *file = (VOID *)(UINTN)(racine + 4096);
  volatile UINT32 *recu = (VOID *)(UINTN)(racine + 8192);
  file[0] = 0x11;  // Invalidation globale des contextes.
  file[2] = 0xd2;  // IOTLB globale avec drainage lecture/écriture.
  file[4] = (1ULL << 32) | 0x25;  // WAIT avec écriture du reçu 1.
  file[5] = racine + 8192;
  if (!fermer_pairs(bs)) fin("pairs-non-draines");
  static UINT64 carte[4096];
  EFI_STATUS sortie = EFI_ABORTED;
  for (UINTN tentative = 0; tentative < 2; ++tentative) {
    UINTN taille = sizeof(carte), cle, pas;
    UINT32 version;
    if (bs->GetMemoryMap(&taille, (VOID *)carte, &cle, &pas, &version) != EFI_SUCCESS) break;
    sortie = bs->ExitBootServices(image, cle);
    if (sortie != EFI_INVALID_PARAMETER) break;
  }
  if (sortie != EFI_SUCCESS) fin("sortie-uefi-refusee");
  __asm__ volatile ("cli");
  if (!topologie(TRUE) || REG32(0x1c) || REG64(0x20) || REG32(0x34)) fin("reprise-refusee");
  for (UINTN offset = 0; offset < 12288; offset += line)
    __asm__ volatile ("clflush (%0)" : : "r"(racine+offset) : "memory");
  __asm__ volatile ("mfence" ::: "memory");
  REG64(0x20) = racine;
  REG32(0x18) = ROOT_OK;
  if (!attendre32(0x1c, ROOT_OK) || REG64(0x20) != racine) fin("racine-non-acquittee");
  REG64(0x90) = racine + 4096;
  REG32(0x18) = QI_ON;
  if (!attendre32(0x1c, ROOT_OK|QI_ON) || REG64(0x90) != racine+4096) fin("qi-non-acquittee");
  REG64(0x88) = 48;
  for (UINTN i = 0; i < 1000000 && *recu != 1 && !REG32(0x34); ++i)
    __asm__ volatile ("pause" ::: "memory");
  if (*recu != 1 || REG64(0x80) != 48 || REG32(0x34)) fin("invalidations-non-acquittees");
  REG32(0x18) = QI_ON|TES;
  if (!attendre32(0x1c, ROOT_OK|QI_ON|TES) || !topologie(TRUE) || REG32(0x34))
    fin("traduction-non-confirmee");
  ligne("NET VTD root="); nombre(racine);
  ligne(" gsts="); nombre(REG32(0x1c)); ligne(" recu="); nombre(*recu); ligne("\n");
  fin("confine");
}
