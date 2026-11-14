//! Interfaces utilisées par le noyau indépendamment de la machine.
pub mod memory;

pub(crate) use crate::arch::{context, init, keyboard, paging, timer};

pub(crate) fn log(args: core::fmt::Arguments<'_>) {
    let _critical = interrupts::Guard::new();
    static WRITING: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
    use core::sync::atomic::Ordering;
    while WRITING
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        crate::arch::poll_translation();
        core::hint::spin_loop();
    }
    if let Some(write) = crate::arch::DIAGNOSTIC {
        write(args);
    }
    WRITING.store(false, Ordering::Release);
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
        #[allow(dead_code)] // Également utilisé par le profil IRQ de qualification.
        pub fn was_enabled(&self) -> bool {
            super::context::irq_enabled(self.0)
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
    pub(crate) use crate::arch::cpu_id as id;

    /// Arrête le processeur courant. Un réveil parasite le remet en veille.
    pub fn halt() -> ! {
        crate::smp::shutdown("normal", format_args!(""))
    }
    pub(crate) fn park_local() -> ! {
        loop {
            super::interrupts::mask();
            crate::arch::wait();
        }
    }
}
