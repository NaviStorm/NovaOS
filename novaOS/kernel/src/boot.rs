use limine::{
    BaseRevision,
    framebuffer::Framebuffer,
    memory_map::Entry,
    paging::Mode,
    request::{
        ExecutableAddressRequest, ExecutableCmdlineRequest, FramebufferRequest, HhdmRequest,
        MemoryMapRequest, PagingModeRequest, RequestsEndMarker, RequestsStartMarker,
    },
};

// Sûreté : le script de liaison place ce marqueur avant toutes les requêtes.
#[unsafe(link_section = ".limine_requests_start")]
#[used]
static START: RequestsStartMarker = RequestsStartMarker::new();

// Sûreté : section alignée et inscriptible pendant le passage de relais par Limine.
#[unsafe(link_section = ".limine_requests")]
#[used]
static BASE: BaseRevision = BaseRevision::with_revision(3);

// Sûreté : cette requête de durée de vie statique réside entre les marqueurs.
#[unsafe(link_section = ".limine_requests")]
#[used]
static FRAMEBUFFER: FramebufferRequest = FramebufferRequest::new();

// Sûreté : le script de liaison place ce marqueur après la dernière requête.
#[unsafe(link_section = ".limine_requests_end")]
#[used]
static END: RequestsEndMarker = RequestsEndMarker::new();

fn framebuffer() -> Result<Framebuffer<'static>, &'static str> {
    if !BASE.is_supported() {
        return Err("Erreur : révision du protocole Limine incompatible.");
    }
    FRAMEBUFFER
        .get_response()
        .and_then(|response| response.framebuffers().next())
        .ok_or("Erreur : aucun framebuffer transmis par Limine.")
}

macro_rules! request {
    ($name:ident: $ty:ty = $value:expr) => {
        // Sûreté : le script de liaison conserve ces requêtes statiques, alignées
        // et inscriptibles par Limine jusqu’au passage de relais au noyau.
        #[unsafe(link_section = ".limine_requests")]
        #[used]
        static $name: $ty = $value;
    };
}

request!(RSDP: limine::request::RsdpRequest = limine::request::RsdpRequest::new());
#[cfg(target_arch = "aarch64")]
request!(DTB: limine::request::DeviceTreeBlobRequest = limine::request::DeviceTreeBlobRequest::new());

#[cfg(target_arch = "aarch64")]
request!(EFI_TYPE: limine::request::FirmwareTypeRequest = limine::request::FirmwareTypeRequest::new());
#[cfg(target_arch = "aarch64")]
request!(EFI_SYSTEM: limine::request::EfiSystemTableRequest = limine::request::EfiSystemTableRequest::new());
#[cfg(target_arch = "aarch64")]
request!(EFI_MAP: limine::request::EfiMemoryMapRequest = limine::request::EfiMemoryMapRequest::new());

#[cfg(target_arch = "aarch64")]
pub(crate) fn efi_tables() -> Option<(u64, &'static limine::response::EfiMemoryMapResponse)> {
    assert!(!RETIRED.load(core::sync::atomic::Ordering::Acquire));
    if BASE.loaded_revision() != Some(3)
        || EFI_TYPE.get_response()?.firmware_type() != limine::firmware_type::FirmwareType::UEFI_64
    {
        return None;
    }
    // Révision 3 : table système physique, copie de la carte EFI dans le HHDM du chargeur.
    Some((
        EFI_SYSTEM.get_response()?.address() as u64,
        EFI_MAP.get_response()?,
    ))
}

pub(crate) fn rsdp_physical() -> Option<u64> {
    assert!(!RETIRED.load(core::sync::atomic::Ordering::Acquire));
    // Limine ne rend une adresse physique pour cette requête qu’en révision de base 3.
    (BASE.loaded_revision() == Some(3))
        .then(|| RSDP.get_response())?
        .map(|r| r.address() as u64)
}

#[cfg(target_arch = "aarch64")]
pub(crate) fn device_tree_address() -> Option<usize> {
    assert!(!RETIRED.load(core::sync::atomic::Ordering::Acquire));
    DTB.get_response().map(|r| r.dtb_ptr() as usize)
}

request!(MAP: MemoryMapRequest = MemoryMapRequest::new());
request!(HHDM: HhdmRequest = HhdmRequest::new());
request!(IMAGE: ExecutableAddressRequest = ExecutableAddressRequest::new());
request!(MODE: PagingModeRequest = PagingModeRequest::new().with_max_mode(Mode::FOUR_LEVEL));
request!(MP: limine::request::MpRequest = limine::request::MpRequest::new());
pub(crate) fn processors() -> Option<&'static limine::response::MpResponse> {
    assert!(!RETIRED.load(core::sync::atomic::Ordering::Acquire));
    MP.get_response()
}

request!(CMDLINE: ExecutableCmdlineRequest = ExecutableCmdlineRequest::new());

pub struct MemoryInfo {
    pub entries: &'static [&'static Entry],
    pub offset: u64,
    pub physical_base: u64,
    pub virtual_base: u64,
}

#[cfg(all(target_arch = "x86_64", feature = "live-video-6a"))]
pub(crate) fn surface_live(heritee: bool) -> Option<crate::hardware::framebuffer::Screen> {
    use limine::memory_map::EntryType;
    if RETIRED.load(core::sync::atomic::Ordering::Acquire) || BASE.loaded_revision() != Some(3) {
        return None;
    }
    let fb = framebuffer().ok()?;
    let s = crate::hardware::framebuffer::Screen::from_limine(&fb).ok()?;
    let (base, bytes) = s.span();
    let offset = HHDM.get_response()?.offset();
    let pa = (base as u64).checked_sub(offset)?;
    let fin = pa.checked_add(bytes as u64)?;
    let map = MAP.get_response()?.entries();
    if offset < 0xffff800000000000 || base % 4096 != 0 || bytes > 128 * 1024 * 1024
        || map.len() > 4096
    {
        return None;
    }
    let mut couvert = false;
    for e in map {
        let end = e.base.checked_add(e.length)?;
        if e.base < fin && pa < end {
            if couvert || e.entry_type != EntryType::FRAMEBUFFER || e.base > pa || end < fin {
                return None;
            }
            couvert = true;
        }
    }
    if !couvert {
        return None;
    }
    if heritee {
        // Ce parcours lit seulement la carte et le HHDM. L'image peut manquer au relais.
        let info = MemoryInfo { entries: map, offset, physical_base: 0, virtual_base: 0 };
        crate::memory::surface_heritee(info, base as u64, pa, bytes)?;
    }
    Some(s)
}

pub fn memory() -> Result<MemoryInfo, &'static str> {
    if RETIRED.load(core::sync::atomic::Ordering::Acquire) {
        return Err("Chargeur déjà libéré.");
    }
    if !BASE.is_supported()
        || !MODE
            .get_response()
            .is_some_and(|r| r.mode() == Mode::FOUR_LEVEL)
    {
        return Err("Pagination Limine incompatible.");
    }
    let image = IMAGE.get_response().ok_or("Adresse du noyau absente.")?;
    Ok(MemoryInfo {
        entries: MAP
            .get_response()
            .ok_or("Carte mémoire absente.")?
            .entries(),
        offset: HHDM.get_response().ok_or("HHDM absent.")?.offset(),
        physical_base: image.physical_base(),
        virtual_base: image.virtual_base(),
    })
}

pub fn scenario() -> &'static [u8] {
    assert!(!RETIRED.load(core::sync::atomic::Ordering::Acquire));
    CMDLINE
        .get_response()
        .map_or(&[], |r| r.cmdline().to_bytes())
}

static RETIRED: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// Dernière lecture des réponses Limine. La géométrie vidéo est possédée par
/// Screen ; seuls les pixels du framebuffer dédié restent utilisés.
pub(crate) fn handoff() -> (
    alloc::vec::Vec<u8>,
    Result<crate::hardware::framebuffer::Screen, &'static str>,
) {
    let args = scenario().to_vec();
    #[cfg(all(target_arch = "x86_64", feature = "live-video-6a"))]
    let screen = surface_live(false).ok_or("Surface Live indisponible.");
    #[cfg(not(all(target_arch = "x86_64", feature = "live-video-6a")))]
    let screen = framebuffer().and_then(|f| crate::hardware::framebuffer::Screen::from_limine(&f));
    RETIRED.store(true, core::sync::atomic::Ordering::Release);
    (args, screen)
}
