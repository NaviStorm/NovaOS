//! Interfaces utilisées par le noyau indépendamment de la machine.
pub mod memory;

pub(crate) use crate::arch::{init, keyboard, paging};

pub(crate) fn log(args: core::fmt::Arguments<'_>) {
    if let Some(write) = crate::arch::DIAGNOSTIC {
        write(args);
    }
}

pub mod interrupts {
    pub(crate) use crate::arch::{
        restore_interrupts as restore, save_and_mask_interrupts as save_and_mask,
    };

    /// Masque les interruptions matérielles masquables du processeur courant.
    pub fn mask() {
        crate::arch::mask_interrupts();
    }
}

pub mod cpu {
    /// Arrête le processeur courant. Un réveil parasite le remet en veille.
    pub fn halt() -> ! {
        loop {
            super::interrupts::mask();
            crate::arch::wait();
        }
    }
}
