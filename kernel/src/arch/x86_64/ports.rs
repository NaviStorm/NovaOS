use core::arch::asm;

#[derive(Clone, Copy)]
#[repr(u16)]
pub(super) enum Port {
    PicCommand = 0x20,
    PicMask = 0x21,
    KeyboardData = 0x60,
    Controller = 0x64,
    SlaveCommand = 0xa0,
    SlaveMask = 0xa1,
}

impl Port {
    pub fn read(self) -> u8 {
        let value: u8;
        // Sûreté : seuls les ports i8042 et 8259 de la machine Q35 sont admis,
        // depuis le noyau en anneau 0. Le contrôleur n’a qu’un lecteur.
        unsafe {
            asm!("in al, dx", in("dx") self as u16, out("al") value, options(nostack, preserves_flags))
        };
        value
    }

    pub fn write(self, value: u8) {
        // Sûreté : l’énumération borne les ports aux deux contrôleurs réservés
        // au noyau. Les pilotes en sérialisent la programmation en anneau 0.
        unsafe {
            asm!("out dx, al", in("dx") self as u16, in("al") value, options(nostack, preserves_flags))
        };
    }
}
