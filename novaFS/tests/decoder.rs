//! Corpus de mutations déterministes avec empreinte recalculée, pour atteindre
//! le décodeur après son contrôle cryptographique.
use novafs::{
    Ids,
    tree::{Family, Node},
};
use sha2::{Digest, Sha256};
#[test]
fn dix_mille_mutations_de_cellules_resignees() {
    let ids = Ids {
        fs: [9; 16],
        lv: [53; 16],
    };
    let n = Node {
        family: Family::Names,
        level: 0,
        entries: (0..24)
            .map(|i: u64| {
                let name = format!("e{i:02}");
                let mut k = 1u64.to_le_bytes().to_vec();
                k.extend_from_slice(&(name.len() as u16).to_le_bytes());
                k.extend_from_slice(name.as_bytes());
                (k, (i + 2).to_le_bytes().to_vec())
            })
            .collect(),
    };
    let (mut reference, original) = n.encode(ids, 20, 8).unwrap();
    let mut seed = 0xaee5_31da_628c_u64;
    let mut rejects = 0;
    let mut accepted = 0;
    for i in 0..10000 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let mut block = original;
        let index = match i % 3 {
            0 => 96 + (seed as usize % 56),
            1 => 152 + (seed as usize % (n.encoded_len() - 152)),
            _ => seed as usize % 4096,
        };
        block[index] ^= 1 << ((seed >> 24) % 8);
        block[64..96].fill(0);
        let mut h = Sha256::new();
        h.update(b"NovaFS.P0.meta\0");
        h.update(block);
        reference.hash = h.finalize().into();
        block[64..96].copy_from_slice(&reference.hash);
        match Node::decode(&block, ids, reference, Family::Names) {
            Ok(parsed) => {
                let (r, reencoded) = parsed.encode(ids, 20, 8).unwrap();
                assert_eq!(r, reference);
                assert_eq!(reencoded, block);
                accepted += 1;
            }
            Err(_) => rejects += 1,
        }
    }
    assert!(rejects > 6000 && accepted > 100);
    println!(
        "Corpus de cellules : {rejects} rejets et {accepted} encodages structurellement canoniques."
    );
}
