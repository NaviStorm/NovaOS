use super::engine::{Error, Word, cat, quoted};
// Numérotation du profil de référence Darwin, indépendante des sélecteurs NovaOS.
pub const NAMES: [&str; 32] = [
    "EXIT", "HUP", "INT", "QUIT", "ILL", "TRAP", "ABRT", "EMT", "FPE", "KILL", "BUS", "SEGV",
    "SYS", "PIPE", "ALRM", "TERM", "URG", "STOP", "TSTP", "CONT", "CHLD", "TTIN", "TTOU", "IO",
    "XCPU", "XFSZ", "VTALRM", "PROF", "WINCH", "INFO", "USR1", "USR2",
];

#[derive(Default)]
pub struct Signals {
    // (ignorer, ignorer les erreurs pour la forme sans argument)
    pub handler: [Option<(bool, bool)>; 32],
    pub block: [Option<bool>; 32],
    pub list: bool,
}
impl Signals {
    pub fn option(&mut self, kind: u8, value: Option<&[u8]>) -> Result<(), Error> {
        let mut set = |n: usize| {
            if kind != b'b' {
                self.handler[n] = Some((kind == b'g', value.is_none()));
            }
            if kind != b'g' {
                self.block[n] = Some(kind == b'b');
            }
        };
        if let Some(value) = value {
            for item in value.split(|c| *c == b',').filter(|x| !x.is_empty()) {
                let upper: Word = item.to_ascii_uppercase();
                let name = upper.strip_prefix(b"SIG").unwrap_or(&upper);
                let number = if item[0].is_ascii_digit() {
                    core::str::from_utf8(item)
                        .ok()
                        .and_then(|s| s.parse::<i32>().ok())
                        .map(|i| (i & if i >= 255 { 255 } else { 127 }) as usize)
                } else if upper.starts_with(b"SIG") && name.iter().all(u8::is_ascii_digit) {
                    core::str::from_utf8(name).ok().and_then(|s| s.parse().ok())
                } else if name == b"IOT" {
                    Some(6)
                } else {
                    NAMES.iter().position(|s| s.as_bytes() == name)
                };
                let n = number
                    .filter(|n| (1..32).contains(n))
                    .ok_or_else(|| Error::usage(cat(&[&quoted(item), b": invalid signal"])))?;
                set(n);
            }
        } else {
            for n in 1..32 {
                set(n);
            }
        }
        Ok(())
    }
}
