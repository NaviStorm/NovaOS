//! Interfaces utilisées par le noyau indépendamment de la machine.
pub mod memory;

pub(crate) use crate::arch::{context, init, keyboard, paging, timer};

pub(crate) fn log(args: core::fmt::Arguments<'_>) {
    let _critical = interrupts::Guard::new();
    if let Some(write) = crate::arch::DIAGNOSTIC {
        write(args);
    }
}

pub mod interrupts {
    pub(crate) use crate::arch::{
        restore_interrupts as restore, save_and_mask_interrupts as save_and_mask,
    };

    /// Section critique locale, imbriquable. La garde doit rester sur son CPU.
    pub(crate) struct Guard(usize, core::marker::PhantomData<*mut ()>);
    impl Guard {
        pub fn new() -> Self {
            Self(save_and_mask(), core::marker::PhantomData)
        }
        pub fn was_enabled(&self) -> bool {
            crate::arch::context::irq_enabled(self.0)
        }
    }
    impl Drop for Guard {
        fn drop(&mut self) {
            restore(self.0);
        }
    }

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
