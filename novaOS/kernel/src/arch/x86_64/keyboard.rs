use super::{interrupts, ports::Port};
use pc_keyboard::{DecodedKey, HandleControl, KeyCode, PS2Keyboard, ScancodeSet2, layouts::Azerty};

static INPUT: crate::sched::Event = crate::sched::Event::new();

const OUTPUT_FULL: u8 = 1;
const INPUT_FULL: u8 = 2;
const AUX_DATA: u8 = 1 << 5;
const TIMEOUT: &str = "Erreur : délai dépassé sur le contrôleur PS/2.";

fn wait_for(mask: u8, expected: u8) -> Result<(), &'static str> {
    for _ in 0..100_000 {
        if Port::Controller.read() & mask == expected {
            return Ok(());
        }
        core::hint::spin_loop();
    }
    Err(TIMEOUT)
}

fn send(port: Port, value: u8) -> Result<(), &'static str> {
    wait_for(INPUT_FULL, 0)?;
    port.write(value);
    Ok(())
}

fn response() -> Result<u8, &'static str> {
    wait_for(OUTPUT_FULL, OUTPUT_FULL)?;
    Ok(Port::KeyboardData.read())
}

fn keyboard_command(byte: u8) -> Result<(), &'static str> {
    for _ in 0..3 {
        send(Port::KeyboardData, byte)?;
        // Une frappe déjà engagée par le firmware peut précéder le premier ACK.
        for _ in 0..32 {
            match response()? {
                0xfa => return Ok(()),
                0xfe => break, // RESEND
                0xfc | 0xfd => return Err("Erreur matérielle du clavier PS/2."),
                _ => continue,
            }
        }
    }
    Err("Erreur : commande refusée par le clavier PS/2.")
}

pub struct Keyboard {
    decoder: PS2Keyboard<Azerty, ScancodeSet2>,
    accent: Option<char>,
    following: Option<char>,
}

impl Keyboard {
    pub fn init() -> Result<Self, &'static str> {
        let _irq = crate::hal::interrupts::Guard::new();
        send(Port::Controller, 0xad)?; // Suspendre les deux ports avant la purge.
        send(Port::Controller, 0xa7)?;
        for _ in 0..256 {
            if Port::Controller.read() & OUTPUT_FULL == 0 {
                break;
            }
            Port::KeyboardData.read();
        }
        wait_for(OUTPUT_FULL, 0)?;
        send(Port::Controller, 0x20)?;
        let config = response()? & !0x43; // Traduction et IRQ des deux ports désactivées.
        send(Port::Controller, 0x60)?;
        send(Port::KeyboardData, config)?;
        send(Port::Controller, 0xae)?; // Activer l’horloge du premier port.
        keyboard_command(0xf5)?;
        keyboard_command(0xf0)?;
        keyboard_command(2)?; // Jeu de scancodes natif 2.
        keyboard_command(0xf4)?; // Activer l’émission.
        send(Port::Controller, 0x60)?;
        send(Port::KeyboardData, (config & !0x10) | 1)?;
        interrupts::allow_keyboard();
        Ok(Self {
            decoder: PS2Keyboard::new(
                ScancodeSet2::new(),
                Azerty,
                HandleControl::MapLettersToUnicode,
            ),
            accent: None,
            following: None,
        })
    }

    fn decode(&mut self, byte: u8) -> Option<char> {
        let event = self.decoder.add_byte(byte).ok()??;
        let accent_key = event.code == KeyCode::Oem4;
        let DecodedKey::Unicode(ch) = self.decoder.process_keyevent(event)? else {
            return None;
        };
        if accent_key && matches!(ch, '^' | '¨') {
            return self.accent.replace(ch);
        }
        let Some(accent) = self.accent.take() else {
            return Some(ch);
        };
        match ch {
            ' ' => Some(accent),
            '\u{8}' => None,
            _ => {
                let vowels = if accent == '^' {
                    "âêîôûÂÊÎÔÛ"
                } else {
                    "äëïöüÄËÏÖÜÿŸ"
                };
                if let Some((_, composed)) = "aeiouAEIOUyY"
                    .chars()
                    .zip(vowels.chars())
                    .find(|&(letter, _)| letter == ch)
                {
                    Some(composed)
                } else {
                    self.following = Some(ch);
                    Some(accent)
                }
            }
        }
    }

    pub fn next_char(&mut self) -> char {
        if let Some(ch) = self.following.take() {
            return ch;
        }
        loop {
            let irq = crate::hal::interrupts::Guard::new();
            let status = Port::Controller.read();
            if status & OUTPUT_FULL == 0 {
                INPUT.wait(&irq);
                continue;
            }
            let byte = Port::KeyboardData.read();
            drop(irq);
            if status & (AUX_DATA | 0xc0) != 0 {
                continue;
            }
            if let Some(ch) = self.decode(byte) {
                return ch;
            }
        }
    }
}

pub(super) extern "C" fn irq() {
    Port::PicCommand.write(0x20);
    INPUT.notify_one();
}
