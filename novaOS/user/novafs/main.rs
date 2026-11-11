#![no_std]
#![no_main]
extern crate alloc;
mod device;
mod heap;
#[path = "../storage/medium.rs"]
mod medium;
mod probes;
#[path = "../storage/runtime.rs"]
mod runtime;
use alloc::vec;
use novafs::{
    Ids,
    tree::{Family, Node},
};
use runtime::*;

fn initialize() -> Result<()> {
    // Aller-retour en mémoire dans les codecs du moteur avant l'ouverture de la session.
    let ids = Ids {
        fs: [1; 16],
        lv: [2; 16],
    };
    let node = Node {
        family: Family::Objects,
        level: 0,
        entries: vec![(1u64.to_le_bytes().to_vec(), vec![11; 32])],
    };
    let (reference, page) = node.encode(ids, 7, 1).map_err(|_| REFUSED)?;
    let copy = Node::decode(&page, ids, reference, node.family).map_err(|_| REFUSED)?;
    if copy.entries != node.entries {
        return Err(REFUSED);
    }
    // Un service dépourvu de CSPRNG doit refuser toute création de secret.
    use novafs::crypto::{Entropy, HostEntropy};
    if !matches!(
        HostEntropy.fill(&mut [0u8; 24]),
        Err(novafs::Error::Unavailable)
    ) {
        return Err(REFUSED);
    }
    let message = b"NVFS coeur initialise ELF tas prive";
    value(call(
        DEBUG_WRITE,
        message.as_ptr() as u64,
        message.len() as u64,
        0,
    ))?;
    Ok(())
}
// Sûreté : symbole d'entrée appelé avec la pile privée initialisée par _start.
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    let mode = config(1);
    if (101..=111).contains(&mode) {
        exit(probes::run(mode).err().unwrap_or(0))
    }
    exit(
        initialize()
            .and_then(|_| medium::serve())
            .err()
            .unwrap_or(0),
    )
}
