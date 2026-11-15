//! Sondes compilées uniquement dans l'ELF du banc de coupures.
use crate::{file, journal_wire as w, runtime as rt};
#[derive(Default)]
pub struct Probe {
    offline: bool,
    fail_rotate: bool,
}
impl Probe {
    pub fn after_ack(&mut self, marker: u64) {
        match marker {
            244 => self.fail_rotate = true,
            245 => self.offline = true,
            _ => (),
        }
    }
    pub fn check_write(&self) -> rt::Result<()> {
        if self.offline { Err(file::IO) } else { Ok(()) }
    }
    pub fn check_admin(&mut self, op: u64) -> rt::Result<()> {
        self.check_write()?;
        if op == w::ROTATE && self.fail_rotate {
            self.fail_rotate = false;
            return Err(file::IO);
        }
        Ok(())
    }
}
pub fn crash() -> ! {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        core::arch::asm!("ud2", options(noreturn));
    }
    #[cfg(target_arch = "aarch64")]
    unsafe {
        core::arch::asm!("brk #0", options(noreturn));
    }
}
