use core::{arch::asm, fmt};

const BASE: u16 = 0x3f8;

fn write_register(offset: u16, byte: u8) {
    // Sûreté : ce module utilise exclusivement les registres du 16550 COM1,
    // depuis le processeur d’amorçage au niveau de privilège 0.
    unsafe {
        asm!("out dx, al", in("dx") BASE + offset, in("al") byte,
             options(nomem, nostack, preserves_flags));
    }
}

pub fn init() {
    write_register(1, 0x00); // Désactiver les interruptions série.
    write_register(3, 0x80); // Accéder au diviseur de débit.
    write_register(0, 0x01); // 115200 bauds.
    write_register(1, 0x00);
    write_register(3, 0x03); // 8 bits, aucune parité, un bit d’arrêt.
    write_register(2, 0xc7); // Activer et vider les FIFO.
    write_register(4, 0x03); // DTR et RTS.
}

fn transmit(byte: u8) -> fmt::Result {
    for _ in 0..65_536 {
        let status: u8;
        // Sûreté : lecture du registre d’état COM1, au niveau de privilège 0.
        unsafe {
            asm!("in al, dx", in("dx") BASE + 5, out("al") status,
                 options(nomem, nostack, preserves_flags));
        }
        if status == 0xff {
            break;
        }
        if status & 0x20 != 0 {
            write_register(0, byte);
            return Ok(());
        }
        core::hint::spin_loop();
    }
    Err(fmt::Error)
}

pub struct Com1;

impl fmt::Write for Com1 {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        text.bytes().try_for_each(transmit)
    }
}

pub fn line(text: fmt::Arguments<'_>) {
    use fmt::Write;
    let _ = writeln!(Com1, "{text}\r");
}
