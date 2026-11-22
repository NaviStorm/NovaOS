//! Données publiques du provisionneur, incluses dans la mesure de l'image du registre.
//! Les clés privées ne sont jamais transmises au registre ni au paquet.
use crate::runtime as rt;
const CATALOG: &[u8] = &[78, 65, 82, 50, 70, 65, 67, 49, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 2, 0, 0, 0, 0, 0, 0, 0, 244, 215, 146, 124, 59, 62, 90, 8, 12, 211, 165, 187, 114, 104, 50, 61, 138, 82, 151, 30, 114, 92, 43, 176, 201, 21, 91, 244, 148, 217, 140, 211, 76, 137, 47, 107, 253, 61, 139, 47, 171, 13, 245, 117, 44, 199, 180, 75, 7, 16, 18, 251, 142, 226, 95, 202, 172, 208, 84, 133, 36, 80, 109, 122];
const EXPECTED: [u8;32] = [70, 216, 94, 13, 176, 212, 245, 128, 130, 185, 46, 215, 11, 88, 84, 105, 57, 190, 44, 217, 170, 168, 69, 107, 50, 133, 104, 108, 178, 72, 245, 207];
pub fn authorize(domain: [u8;16], reference: [u8;16]) -> rt::Result<()> {
    let catalogue=novaos_identity::enrollment_catalog::Catalog::parse(CATALOG,EXPECTED,domain)
        .map_err(|_|rt::DENIED)?;
    if catalogue.allows(reference) {Ok(())} else {Err(rt::DENIED)}
}
