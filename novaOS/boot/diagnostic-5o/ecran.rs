use crate::{AUTORITE, Resultat, donnees, firmware};
use core::{mem::size_of, ptr};
use ed25519_dalek::{Signature, VerifyingKey};
use r_efi::{efi, protocols::graphics_output as gop};

pub const LARGEUR: usize = 544;
pub const HAUTEUR: usize = 120;
const CORPS: usize = 104;
const TAILLE: usize = CORPS + 64;

// Alphabet 5×7. Chaque ligne tient dans les cinq bits de poids faible.
fn glyphe(c: u8) -> [u8; 7] {
    match c {
        b'A' => [14, 17, 17, 31, 17, 17, 17],
        b'B' => [30, 17, 17, 30, 17, 17, 30],
        b'C' => [14, 17, 16, 16, 16, 17, 14],
        b'D' => [30, 17, 17, 17, 17, 17, 30],
        b'E' => [31, 16, 16, 30, 16, 16, 31],
        b'F' => [31, 16, 16, 30, 16, 16, 16],
        b'G' => [14, 17, 16, 23, 17, 17, 15],
        b'H' => [17, 17, 17, 31, 17, 17, 17],
        b'I' => [14, 4, 4, 4, 4, 4, 14],
        b'J' => [7, 2, 2, 2, 18, 18, 12],
        b'K' => [17, 18, 20, 24, 20, 18, 17],
        b'L' => [16, 16, 16, 16, 16, 16, 31],
        b'M' => [17, 27, 21, 21, 17, 17, 17],
        b'N' => [17, 25, 21, 19, 17, 17, 17],
        b'O' => [14, 17, 17, 17, 17, 17, 14],
        b'P' => [30, 17, 17, 30, 16, 16, 16],
        b'Q' => [14, 17, 17, 17, 21, 18, 13],
        b'R' => [30, 17, 17, 30, 20, 18, 17],
        b'S' => [15, 16, 16, 14, 1, 1, 30],
        b'T' => [31, 4, 4, 4, 4, 4, 4],
        b'U' => [17, 17, 17, 17, 17, 17, 14],
        b'V' => [17, 17, 17, 17, 17, 10, 4],
        b'W' => [17, 17, 17, 21, 21, 21, 10],
        b'X' => [17, 17, 10, 4, 10, 17, 17],
        b'Y' => [17, 17, 10, 4, 4, 4, 4],
        b'Z' => [31, 1, 2, 4, 8, 16, 31],
        b'4' => [2, 6, 10, 18, 31, 2, 2],
        b'0' => [14, 17, 19, 21, 25, 17, 14],
        _ => [0; 7],
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Surface {
    base: u64,
    taille: u64,
    largeur: u32,
    hauteur: u32,
    pas: u32,
    format: u32,
}

impl Surface {
    fn admise(self) -> bool {
        self.base != 0
            && self.base % 4 == 0
            && self.taille <= 128 << 20
            && self
                .base
                .checked_add(self.taille)
                .is_some_and(|fin| fin <= 1 << 47)
            && (LARGEUR as u32..=8192).contains(&self.largeur)
            && (HAUTEUR as u32..=8192).contains(&self.hauteur)
            && (self.largeur..=16384).contains(&self.pas)
            && self.format <= 1
            && u64::from(self.hauteur) * u64::from(self.pas) * 4 <= self.taille
    }
}

fn objet<T: Copy>(p: *const T, carte: &firmware::Map) -> Result<T, u64> {
    if !carte.covers(p as u64, size_of::<T>(), donnees) {
        return Err(43);
    }
    Ok(unsafe { ptr::read_unaligned(p) })
}

fn console(st: &efi::SystemTable, carte: &firmware::Map) -> Result<Surface, u64> {
    let bs = unsafe { st.boot_services.as_ref() }.ok_or(43u64)?;
    let mut p = ptr::null_mut();
    let status =
        unsafe { (bs.handle_protocol)(st.console_out_handle, &mut { gop::PROTOCOL_GUID }, &mut p) };
    if status != efi::Status::SUCCESS
        || !carte.covers(p as u64, size_of::<gop::Protocol>(), donnees)
    {
        return Err(43);
    }
    let protocol = p.cast::<gop::Protocol>();
    let mode = objet(
        unsafe { ptr::read_unaligned(&raw const (*protocol).mode).cast_const() },
        carte,
    )?;
    if mode.mode >= mode.max_mode || mode.size_of_info < size_of::<gop::ModeInformation>() {
        return Err(43);
    }
    let i = objet(mode.info, carte)?;
    if i.version != 0 {
        return Err(43);
    }
    Ok(Surface {
        base: mode.frame_buffer_base,
        taille: mode.frame_buffer_size as u64,
        largeur: i.horizontal_resolution,
        hauteur: i.vertical_resolution,
        pas: i.pixels_per_scan_line,
        format: i.pixel_format,
    })
}

fn rendu(mode: u64, mut pixel: impl FnMut(usize, usize, u32)) {
    let lignes: [&[u8]; 5] = [
        b"NOVAOS   REFUS 40",
        if mode == 1 {
            b"SECURE BOOT  ACTIF"
        } else {
            b"SECURE BOOT  INACTIF"
        },
        b"CONFIANCE  RECU DE LABORATOIRE",
        b"DMA  PROTECTION ETABLIE AU BANC",
        b"MOTIF  ORIGINE USB NON ATTESTEE",
    ];
    for y in 0..HAUTEUR {
        for x in 0..LARGEUR {
            let mut couleur = 0x00161616;
            if x >= 12 && y >= 6 {
                let (ligne, dy) = ((y - 6) / 22, (y - 6) % 22);
                let (col, dx) = ((x - 12) / 12, (x - 12) % 12);
                if let Some(c) = lignes.get(ligne).and_then(|l| l.get(col))
                    && dy < 14
                    && dx < 10
                    && glyphe(*c)[dy / 2] & (16 >> (dx / 2)) != 0
                {
                    couleur = 0x00ffffff;
                }
            }
            pixel(x, y, couleur);
        }
    }
}

// Le reçu séparé engage la surface, le PE et l'époque déjà authentifiés par 5M.
// Seul le texte d'un refus de l'origine USB est publiable dans cette version.
pub(super) fn afficher(
    st: &efi::SystemTable,
    carte: &firmware::Map,
    r: &Resultat,
    prefixe: &[u8],
) -> Result<(), u64> {
    let rec = prefixe
        .get(prefixe.len().checked_sub(TAILLE).ok_or(41u64)?..)
        .ok_or(41u64)?;
    if &rec[..16] != b"NOVA5O-DISPLAY1!"
        || rec[16..32] != r.epoque
        || rec[32..64] != r.image
        || rec[96..104] != [r.confiance as u8, 1, 1, 40, 0, 0, 0, 0]
    {
        return Err(41);
    }
    VerifyingKey::from_bytes(AUTORITE)
        .map_err(|_| 42u64)?
        .verify_strict(
            &rec[..CORPS],
            &Signature::from_bytes(rec[CORPS..].try_into().unwrap()),
        )
        .map_err(|_| 42u64)?;
    let mot = |n: usize| u32::from_le_bytes(rec[n..n + 4].try_into().unwrap());
    let surface = Surface {
        base: u64::from_le_bytes(rec[64..72].try_into().unwrap()),
        taille: u64::from_le_bytes(rec[72..80].try_into().unwrap()),
        largeur: mot(80),
        hauteur: mot(84),
        pas: mot(88),
        format: mot(92),
    };
    if !surface.admise() || console(st, carte)? != surface {
        return Err(44);
    }
    // x et y sont bornés par rendu. admise couvre jusqu'à la dernière ligne du pas.
    rendu(r.confiance, |x, y, c| unsafe {
        ptr::write_volatile(
            (surface.base as usize + 4 * (y * surface.pas as usize + x)) as *mut u32,
            c,
        )
    });
    Err(40)
}
