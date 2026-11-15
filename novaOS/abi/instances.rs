//! Extension v1 additive. INFO ne consulte que l’appartenance de l’appelant.
include!("generated/instances.rs");

// Identités attribuées dans le manifeste immuable de la distribution embarquée.
// Les identités des services internes désignent le paquet système.
pub const SYSTEM: u64 = 0x4e4f_5641_0000_0001;
pub const SHELL: u64 = 0x4e4f_5641_0000_0010;
pub const WITNESS: u64 = 0x4e4f_5641_ffff_0001;
pub const SAMPLE: u64 = 0x4e4f_5641_0000_0020;
pub const ALLOW: u64 = 1;

/// Indice du programme de référence de l’installation embarquée.
pub const SAMPLE_INDEX: usize = 16;

/// Identité publique, sans autorité. Les octets du BootID sont opaques. Les
/// deux entiers sont little-endian. Les trois champs forment un seul message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Identity {
    pub boot: [u8; 16],
    pub instance: u64,
    pub generation: u64,
}

impl Identity {
    pub const SIZE: u64 = 32;

    pub fn encode(self) -> [u8; Self::SIZE as usize] {
        let mut bytes = [0; Self::SIZE as usize];
        bytes[..16].copy_from_slice(&self.boot);
        bytes[16..24].copy_from_slice(&self.instance.to_le_bytes());
        bytes[24..].copy_from_slice(&self.generation.to_le_bytes());
        bytes
    }

    pub fn decode(bytes: [u8; Self::SIZE as usize]) -> Self {
        Self {
            boot: bytes[..16].try_into().unwrap(),
            instance: u64::from_le_bytes(bytes[16..24].try_into().unwrap()),
            generation: u64::from_le_bytes(bytes[24..].try_into().unwrap()),
        }
    }
}
