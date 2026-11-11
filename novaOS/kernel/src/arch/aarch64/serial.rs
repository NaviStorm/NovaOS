//! PL011 de QEMU virt, accessible uniquement après installation de sa page Device.
use core::{
    fmt::{self, Write},
    sync::atomic::{AtomicBool, AtomicU8, Ordering},
};
pub const VA: u64 = 0xffff_ff00_0000_0000;
pub const PA: u64 = 0x0900_0000;
pub static READY: AtomicBool = AtomicBool::new(false);

struct Uart;
impl Write for Uart {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        for byte in text.bytes() {
            let mut remaining = 65536;
            // Sûreté : READY implique le mappage Device du PL011 de virt, hérité
            // initialisé du firmware. FR et DR sont des registres 32 bits alignés.
            unsafe {
                while ((VA + 0x18) as *const u32).read_volatile() & 32 != 0 {
                    remaining -= 1;
                    if remaining == 0 {
                        return Err(fmt::Error);
                    }
                }
                (VA as *mut u32).write_volatile(byte.into());
            }
        }
        Ok(())
    }
}
// Sûreté : symbole unique en RAM pour le débogueur, y compris si la MMU ne
// peut pas être initialisée. Les écritures octet par octet sont atomiques.
#[unsafe(no_mangle)]
static NOVA_EARLY_DIAGNOSTIC: [AtomicU8; 256] = [const { AtomicU8::new(0) }; 256];

struct EarlyMessage(usize);
impl Write for EarlyMessage {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        for byte in text.bytes().take(NOVA_EARLY_DIAGNOSTIC.len() - 1 - self.0) {
            NOVA_EARLY_DIAGNOSTIC[self.0].store(byte, Ordering::Relaxed);
            self.0 += 1;
        }
        NOVA_EARLY_DIAGNOSTIC[self.0].store(0, Ordering::Release);
        Ok(())
    }
}
pub fn line(args: fmt::Arguments<'_>) {
    if READY.load(Ordering::Acquire) {
        let _ = writeln!(Uart, "{args}\r");
    } else {
        let _ = EarlyMessage(0).write_fmt(args);
    }
}

pub(crate) fn input() -> Option<u8> {
    if !READY.load(Ordering::Acquire) {
        return None;
    }
    // Sûreté : READY garantit la page Device PL011. RXFE protège la lecture de DR.
    unsafe {
        (((VA + 0x18) as *const u32).read_volatile() & 16 == 0)
            .then(|| (VA as *const u32).read_volatile() as u8)
    }
}
pub(crate) fn output(text: &str) -> fmt::Result {
    Uart.write_str(text)
}
