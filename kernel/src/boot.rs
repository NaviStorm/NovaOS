use limine::{
    BaseRevision,
    framebuffer::Framebuffer,
    request::{FramebufferRequest, RequestsEndMarker, RequestsStartMarker},
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

pub fn framebuffer() -> Result<Framebuffer<'static>, &'static str> {
    if !BASE.is_supported() {
        return Err("Erreur : révision du protocole Limine incompatible.");
    }
    FRAMEBUFFER
        .get_response()
        .and_then(|response| response.framebuffers().next())
        .ok_or("Erreur : aucun framebuffer transmis par Limine.")
}
