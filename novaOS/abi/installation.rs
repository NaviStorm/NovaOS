//! Manifeste de l’installation système embarquée. Les identifiants sont attribués
//! indépendamment du chemin, du contenu révisable des ELF et de leur indice CLI.
pub const COMMANDS: &[(&str, u64)] = &[
    ("pwd", 0x4e4f_5641_0000_0101),
    ("ls", 0x4e4f_5641_0000_0102),
    ("cat", 0x4e4f_5641_0000_0103),
    ("head", 0x4e4f_5641_0000_0104),
    ("mkdir", 0x4e4f_5641_0000_0105),
    ("rm", 0x4e4f_5641_0000_0106),
    ("probe", 0x4e4f_5641_0000_0107),
    ("cp", 0x4e4f_5641_0000_0108),
    ("mv", 0x4e4f_5641_0000_0109),
    ("touch", 0x4e4f_5641_0000_010a),
    ("more", 0x4e4f_5641_0000_010b),
    ("wc", 0x4e4f_5641_0000_010c),
    ("tail", 0x4e4f_5641_0000_010d),
    ("rmdir", 0x4e4f_5641_0000_010e),
    ("vi", 0x4e4f_5641_0000_010f),
    ("nsh", 0x4e4f_5641_0000_0010),
];

pub const SHA256SUM: u64 = 0x4e4f_5641_0000_0113;
pub const SHA256SUM_INDEX: usize = 19;

/// Les anciennes applications ne délèguent pas de création de travailleurs.
/// Un nouveau profil multiprocessus doit être admis explicitement ici.
pub fn grant(installation: u64, ports: u64, pair: u64) -> Option<bool> {
    use super::instance_abi as wire;
    if pair > 1 || ports & !7 != 0 {
        return None;
    }
    match installation {
        wire::SAMPLE if ports == 0 && pair == 0 => Some(true),
        wire::WITNESS if ports == 0 && pair == 0 => Some(false),
        SHA256SUM if ports & !6 == 0 => Some(false),
        wire::SHELL => None,
        id if COMMANDS.iter().any(|&(_, app)| app == id) => Some(false),
        _ => None,
    }
}
