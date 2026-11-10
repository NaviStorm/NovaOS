//! Données déterministes communes aux deux Mach-O et à l'oracle hors moteur.
pub(crate) fn suivant(x: &mut u64) -> u64 {
    *x ^= *x << 13;
    *x ^= *x >> 7;
    *x ^= *x << 17;
    *x
}

pub(crate) fn donnees(len: usize, profil: &str, seed: u64) -> Vec<u8> {
    let mut x = seed | 1;
    let mut bytes = vec![0; len];
    for (n, chunk) in bytes.chunks_mut(8).enumerate() {
        let random = suivant(&mut x).to_le_bytes();
        let period = (seed as u8).wrapping_add((n / 512) as u8).max(1);
        match profil {
            "repetitif" => chunk.fill(period),
            "mixte" if n % 512 < 320 => chunk.fill(period),
            "mixte" | "aleatoire" => chunk.copy_from_slice(&random[..chunk.len()]),
            _ => panic!("profil du banc inconnu"),
        }
    }
    bytes
}
