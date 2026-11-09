mod font;

use limine::framebuffer::{Framebuffer, MemoryModel};

pub(crate) struct Screen {
    base: *mut u32,
    pub(crate) width: usize,
    pub(crate) height: usize,
    stride: usize,
    shifts: [u8; 3],
}

impl Screen {
    fn from_limine(fb: &Framebuffer<'_>) -> Result<Self, &'static str> {
        let sizes = [
            fb.red_mask_size(),
            fb.green_mask_size(),
            fb.blue_mask_size(),
        ];
        let shifts = [
            fb.red_mask_shift(),
            fb.green_mask_shift(),
            fb.blue_mask_shift(),
        ];
        if fb.memory_model() != MemoryModel::RGB
            || fb.bpp() != 32
            || sizes != [8; 3]
            || !shifts.iter().all(|s| *s <= 24 && *s % 8 == 0)
            || shifts[0] == shifts[1]
            || shifts[0] == shifts[2]
            || shifts[1] == shifts[2]
        {
            return Err("Erreur : format vidéo incompatible.");
        }
        let width = fb.width() as usize;
        let height = fb.height() as usize;
        let pitch = fb.pitch() as usize;
        let bytes = pitch
            .checked_mul(height)
            .ok_or("Erreur : taille vidéo excessive.")?;
        let base = fb.addr().cast::<u32>();
        if base.is_null()
            || !base.is_aligned()
            || pitch % 4 != 0
            || width == 0
            || height == 0
            || width > pitch / 4
            || bytes > isize::MAX as usize
            || base.addr().checked_add(bytes).is_none()
        {
            return Err("Erreur : géométrie du framebuffer invalide.");
        }
        Ok(Self {
            base,
            width,
            height,
            stride: pitch / 4,
            shifts,
        })
    }

    pub(crate) fn rgb(&self, r: u8, g: u8, b: u8) -> u32 {
        let [rs, gs, bs] = self.shifts;
        (u32::from(r) << rs) | (u32::from(g) << gs) | (u32::from(b) << bs)
    }

    pub(crate) fn fill(
        &mut self,
        left: usize,
        top: usize,
        width: usize,
        height: usize,
        color: u32,
    ) {
        assert!(left <= self.width && width <= self.width - left);
        assert!(top <= self.height && height <= self.height - top);
        for y in top..top + height {
            for x in left..left + width {
                let pixel = self.base.wrapping_add(y * self.stride + x);
                // Sûreté : cette adresse alignée se trouve dans la zone vidéo
                // mappée par Limine. Les bornes du rectangle et le pas sont
                // validés. Seul le processeur d’amorçage écrit dans cette zone.
                unsafe { pixel.write_volatile(color) };
            }
        }
    }

    pub(crate) fn scroll_up(&mut self, top: usize, bottom: usize, step: usize) {
        assert!(top <= bottom && bottom <= self.height && step <= bottom - top);
        for y in top..bottom - step {
            for x in 0..self.width {
                let dst = self.base.wrapping_add(y * self.stride + x);
                let src = self.base.wrapping_add((y + step) * self.stride + x);
                // Sûreté : les deux pixels sont dans les lignes visibles validées
                // et initialisées au démarrage. La copie ascendante protège la source.
                unsafe { dst.write_volatile(src.read_volatile()) };
            }
        }
    }
}

/// Renvoie l’écran initialisé et l’ordonnée qui suit la bannière.
pub fn show_banner(fb: &Framebuffer<'_>, text: &str) -> Result<(Screen, usize), &'static str> {
    let mut screen = Screen::from_limine(fb)?;
    let columns = text
        .len()
        .checked_mul(font::WIDTH + 1)
        .and_then(|width| width.checked_sub(1))
        .ok_or("Erreur : longueur de bannière invalide.")?;
    let scale = 4
        .min(screen.width / columns)
        .min(screen.height / font::HEIGHT);
    if scale == 0 {
        return Err("Erreur : écran trop petit pour la bannière.");
    }

    let background = screen.rgb(16, 24, 39);
    let ink = screen.rgb(232, 241, 251);
    screen.fill(0, 0, screen.width, screen.height, background);

    let left = (screen.width - columns * scale) / 2;
    let top = (screen.height - font::HEIGHT * scale) / 2;
    for (index, byte) in text.bytes().enumerate() {
        let origin = left + index * (font::WIDTH + 1) * scale;
        for (row, bits) in font::glyph(byte).iter().enumerate() {
            for column in 0..font::WIDTH {
                if bits & (1 << (font::WIDTH - 1 - column)) != 0 {
                    screen.fill(
                        origin + column * scale,
                        top + row * scale,
                        scale,
                        scale,
                        ink,
                    );
                }
            }
        }
    }
    Ok((screen, top + font::HEIGHT * scale))
}
