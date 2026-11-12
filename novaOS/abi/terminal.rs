//! Extension additive de l'ABI native. Les numéros du catalogue désignent des ELF statiques.
pub const SPAWN: u64 = 0x10042;
pub const CONSOLE: u64 = 0x10043;
pub const INPUT: u8 = 1;
pub const OUTPUT: u8 = 2;
pub const ERROR: u8 = 4;
/// Style du nom d’un répertoire sur le terminal série, exige OUTPUT.
pub const DIRECTORY_COLOR: u8 = 64;
/// Présentation VT réservée au propriétaire de la session interactive (INPUT | OUTPUT | EDIT).
pub const SCREEN: u8 = 65;
pub const ENTER_SCREEN: u64 = 0;
pub const LEAVE_SCREEN: u64 = 1;
pub const POSITION: u64 = 2;
pub const ERASE_EOL: u64 = 3;
pub const QUERY_SIZE: u64 = 4;
pub const READ_TIMED: u64 = 5;
pub const MAX_AXIS: u16 = 999;
pub const EDIT: u8 = 8;
pub const LAUNCH: u8 = 16;
pub const QUALIFY: u8 = 32;
pub const HEADER: usize = 40;
pub const ARG_MAX: usize = 2560;
pub const LINE_MAX: usize = 2048;
pub const ARG_COUNT: usize = 32;

/// Un scalaire affichable tel quel sur le terminal. Les sauts de ligne sont traités par le flux.
pub fn printable(c: char) -> bool {
    !c.is_control()
        && !matches!(
            c,
            '\u{061c}' | '\u{200e}' | '\u{200f}'
            | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
        )
}
