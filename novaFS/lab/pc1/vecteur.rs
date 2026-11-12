//! Oracle de relecture spécialisé aux deux corps figés du §8, indépendant de l'encodeur.
use hkdf::Hkdf;
use hmac::digest::KeyInit;
use hmac::{Hmac, Mac};
use sha2_p7::{Digest, Sha256};

const IDS: [&str; 2] = [
    "4b05695c5809db3257b3b455e9ab6bb3e0a42f672da65f7b1a16b482df69af1f",
    "61711ad4d14bbaabdbbf5242095cb946fdf7a8d777c2f35cdde4eeaf506b37db",
];

pub fn relire(data: &[u8]) -> (&'static str, usize) {
    assert_eq!(data.len(), 256 * 4096);
    let page = |p: usize| &data[p * 4096..(p + 1) * 4096];
    let mut sel = [1; 48];
    sel[16..32].fill(2);
    sel[32..].fill(3);
    let hk = Hkdf::<Sha256>::new(Some(&sel), &core::array::from_fn::<_, 32, _>(|i| i as u8));
    let mut valide = [[false; 3]; 2];
    let mut trace = [false; 4];
    for g in 0..4 {
        for (role, zone) in [4, 0, 252].into_iter().enumerate() {
            let p = g + zone;
            let bloc = page(p);
            if bloc.iter().all(|&b| b == 0) {
                continue;
            }
            trace[g] = true;
            if g >= 2
                || bloc[208] != role as u8
                || bloc[209..216] != [0; 7]
                || bloc[216..224] != (p as u64).to_le_bytes()
                || bloc[224..4064].iter().any(|&b| b != 0)
            {
                continue;
            }
            let id = Sha256::new()
                .chain_update(b"NovaFS/PC1/id\0")
                .chain_update(&bloc[..208])
                .finalize();
            let ref_id: [u8; 32] = core::array::from_fn(|i| {
                u8::from_str_radix(&IDS[g][2 * i..2 * i + 2], 16).unwrap()
            });
            if id[..] != ref_id {
                continue;
            }
            let label: &[u8] = if role == 0 {
                b"NovaFS/PC1/intent-key\0"
            } else {
                b"NovaFS/PC1/decision-key\0"
            };
            let mut key = [0; 32];
            hk.expand_multi_info(&[label, &[4; 16]], &mut key).unwrap();
            let mut h = Hmac::<Sha256>::new_from_slice(&key).unwrap();
            h.update(b"NovaFS/PC1/page\0");
            h.update(&bloc[..4064]);
            valide[g][role] = h.verify_slice(&bloc[4064..]).is_ok();
        }
    }
    let Some(g) = (0..2).rev().find(|&i| valide[i][1] || valide[i][2]) else {
        return ("refus", 0);
    };
    if !valide[0].iter().any(|&ok| ok) {
        return ("refus", 0);
    }
    let w = page(8 + g);
    if &w[..8] != b"NVTST001"
        || w[8..16] != ((g + 1) as u64).to_le_bytes()
        || w[16..].iter().any(|&b| b != 0)
    {
        return ("refus", 0);
    }
    let etat = if trace[g + 1..].iter().any(|&v| v) {
        "indéterminé"
    } else if valide[..=g].iter().flatten().all(|&v| v) {
        "sain"
    } else {
        "dégradé"
    };
    (etat, g + 1)
}
