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
    let screen = framebuffer().and_then(|f| crate::hardware::framebuffer::Screen::from_limine(&f));
    RETIRED.store(true, core::sync::atomic::Ordering::Release);
    (args, screen)
}
