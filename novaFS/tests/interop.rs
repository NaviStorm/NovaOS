//! F01. Le seul canal d'échange d'image entre exécutables est un pipe.
//! Les octets entrants sont décodés dans un simulateur borné.
use novafs::codec::data_hash;
use novafs::device::SimDevice;
use novafs::{Device, Engine, Ids, Mode, Result};
use sha2::{Digest, Sha256};
use std::io::{self, Read, Write};

const IDS: Ids = Ids {
    fs: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
    lv: [
        17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32,
    ],
};
const BLOCKS: u64 = 384;
const BYTES: usize = BLOCKS as usize * 4096;
const START: &[u8] = b"\x00NOVAFS-F01-DEBUT\xff";
const END: &[u8] = b"\x00NOVAFS-F01-FIN\xff";
const CANON: &str = "/répertoire/é";
const DECOMPOSED: &str = "/répertoire/e\u{301}";

fn original() -> Vec<u8> {
    (0..9000).map(|i| (i % 251) as u8).collect()
}

fn fixture(sector: u32) -> Result<Vec<u8>> {
    let d = SimDevice::new(BLOCKS, sector)?;
    let host = d.control();
    let mut fs = Engine::format(d, IDS)?;
    fs.mkdir("/répertoire")?;
    fs.mkdir("/可")?;
    fs.create(CANON)?;
    fs.create(DECOMPOSED)?;
    fs.write(CANON, 0, &original())?;
    fs.write(CANON, 4094, b"ABCDEF")?;
    fs.write(DECOMPOSED, 8192, b"creux")?;
    fs.rename(DECOMPOSED, "/可/ß")?;
    fs.verify()?;
    drop(fs.unmount()?);
    host.snapshot().bytes()
}

fn inspect(bytes: &[u8], sector: u32) -> Result<()> {
    assert_eq!(bytes.len(), BYTES);
    let d = SimDevice::from_bytes(bytes, sector)?;
    let fs = Engine::mount(d, IDS, Mode::ReadOnly)?;
    fs.verify()?;
    assert_eq!(fs.generation(), 9);
    assert_eq!(fs.list("/", None, 16)?, ["répertoire", "可"]);
    assert_eq!(fs.list("/répertoire", None, 8)?, ["é"]);
    assert!(fs.stat(DECOMPOSED).is_err());
    let mut expected = original();
    expected[4094..4100].copy_from_slice(b"ABCDEF");
    assert_eq!(fs.read(CANON, 0, 20_000)?, expected);
    let mut hole = vec![0; 8192];
    hole.extend(b"creux");
    assert_eq!(fs.read("/可/ß", 0, 19999)?, hole);
    // Oracle indépendant des lecteurs d'enveloppes et des helpers d'encodage.
    let blob = &bytes[4096..8192];
    assert_eq!(&blob[..16], b"NOVAP0FS\0\0\0\0\x01\0\x60\0");
    assert_eq!(&blob[16..32], &IDS.fs);
    assert_eq!(&blob[32..48], &IDS.lv);
    assert_eq!(&blob[48..56], &[1, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(&blob[56..64], &[9, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(
        &blob[96..112],
        &[0x00, 0x10, 0, 0, 0, 0, 0, 0, 0x80, 0x01, 0, 0, 0, 0, 0, 0]
    );
    assert!(blob[184..].iter().all(|b| *b == 0));
    assert_eq!(&blob[64..96], &meta_checksum(blob));
    Ok(())
}

fn meta_checksum(page: &[u8]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"NovaFS.P0.meta\x00");
    digest.update(&page[..64]);
    digest.update([0; 32]);
    digest.update(&page[96..]);
    digest.finalize().into()
}

#[test]
fn f01_vectors_endian_and_features() -> Result<()> {
    let bytes = fixture(512)?;
    let alternate = fixture(4096)?;
    assert_eq!(bytes, alternate);
    inspect(&bytes, 512)?;
    inspect(&bytes, 4096)?;
    // Somme de référence issue de Python hashlib, octets 0..255 répétés 16 fois.
    let mut data = [0; 4096];
    for (i, b) in data.iter_mut().enumerate() {
        *b = i as u8;
    }
    let hex = data_hash(IDS, 0x0102030405060708, 19, &data)
        .map(|b| format!("{b:02x}"))
        .concat();
    assert_eq!(
        hex,
        "683cc12dfe5589ca4de82f5cf8561b35345832f5dc20777c6582bb570103c547"
    );
    // Origine étrangère décalée d'un secteur par rapport à la vue normalisée.
    let mut shifted = bytes.clone();
    shifted.rotate_left(512);
    let bad = SimDevice::from_bytes(&shifted, 512)?;
    assert!(Engine::mount(bad, IDS, Mode::ReadOnly).is_err());
    for mutation in 0..4 {
        let d = SimDevice::from_bytes(&bytes, 512)?;
        let h = d.control();
        let mut page = d.read(1)?;
        match mutation {
            0 => page[8] = 1,              // majeure inconnue
            1 => page[128] = 1,            // fonctionnalité incompatible
            2 => page[104..112].reverse(), // capacité en big-endian
            _ => page[96..100].reverse(),  // taille de bloc en big-endian
        };
        let hash = meta_checksum(&page);
        page[64..96].copy_from_slice(&hash);
        h.replace_stable(1, page)?;
        assert!(Engine::mount(d, IDS, Mode::ReadOnly).is_err());
    }
    Ok(())
}

#[test]
fn f01_pipe_transport() -> Result<()> {
    let sector: u32 = std::env::var("NOVAFS_TEST_SECTOR")
        .unwrap_or_else(|_| "512".into())
        .parse()
        .unwrap();
    match std::env::var("NOVAFS_TEST_PIPE").as_deref() {
        Ok("export") => {
            let bytes = fixture(sector)?;
            let mut out = io::stdout().lock();
            out.write_all(START)?;
            out.write_all(&bytes)?;
            out.write_all(END)?;
            out.flush()?;
        }
        Ok("import") => {
            let mut bytes = vec![0; BYTES];
            let mut input = io::stdin().lock();
            input.read_exact(&mut bytes)?;
            let mut extra = [0];
            if input.read(&mut extra)? != 0 {
                return Err(novafs::Error::Invalid("suffixe du flux F01"));
            }
            inspect(&bytes, sector)?;
        }
        Err(_) => inspect(&fixture(sector)?, sector)?,
        _ => return Err(novafs::Error::Invalid("mode de transport du banc")),
    }
    Ok(())
}
