//! Adaptateur de secrets du laboratoire Darwin. /dev/fd/3 duplique le pipe
//! du parent. Le descripteur et les buffers sont libérés avant tout retour.
use novafs::crypto::{Credential, Secret};
use novafs::{Error, Result};
use std::fs::File;
use std::io::Read;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use zeroize::Zeroizing;

pub(crate) struct KeyFrame {
    pub pool: [u8; 16],
    pub admin: Secret,
    pub domains: Vec<([u8; 16], KeyCredential)>,
}

pub(crate) enum KeyCredential {
    Passphrase(Zeroizing<Vec<u8>>),
    Recovery(Secret),
}

impl KeyCredential {
    pub fn borrowed(&self) -> Credential<'_> {
        match self {
            Self::Passphrase(p) => Credential::Passphrase(p),
            Self::Recovery(k) => Credential::Recovery(k),
        }
    }
}

pub(crate) fn pipe(fd: u8) -> Result<File> {
    let path = match fd {
        3 => "/dev/fd/3",
        4 => "/dev/fd/4",
        _ => return Err(Error::Invalid("numéro de pipe P7")),
    };
    let pipe = File::open(path).map_err(|_| Error::Invalid("pipe anonyme P7 hérité requis"))?;
    let m = pipe.metadata()?;
    if !m.file_type().is_fifo() || m.dev() != 0 || m.nlink() != 0 {
        return Err(Error::Invalid(
            "le descripteur doit être un pipe anonyme Darwin",
        ));
    }
    Ok(pipe)
}

pub(crate) fn read() -> Result<KeyFrame> {
    let malformed = || Error::Invalid("trame de secrets NP7KEYS1 invalide");
    let mut pipe = pipe(3)?;
    let mut read = |b: &mut [u8]| pipe.read_exact(b).map_err(|_| malformed());
    let mut magic = [0; 8];
    read(&mut magic)?;
    if &magic != b"NP7KEYS1" {
        return Err(malformed());
    }
    let mut pool = [0; 16];
    read(&mut pool)?;
    if pool == [0; 16] {
        return Err(malformed());
    }
    let mut key = Zeroizing::new([0u8; 32]);
    read(&mut key[..])?;
    let admin = Secret::from_bytes(*key);
    drop(key);
    let mut count = [0; 1];
    read(&mut count)?;
    if count[0] > 17 {
        return Err(malformed());
    }
    let mut domains = Vec::with_capacity(usize::from(count[0]));
    for _ in 0..count[0] {
        let mut id = [0; 16];
        read(&mut id)?;
        if id == [0; 16] || domains.iter().any(|(d, _)| d == &id) {
            return Err(malformed());
        }
        let mut fields = [0; 3];
        read(&mut fields)?;
        let n = usize::from(u16::from_le_bytes([fields[1], fields[2]]));
        let credential = match fields[0] {
            1 if (1..=1024).contains(&n) => {
                let mut phrase = Zeroizing::new(vec![0u8; n]);
                read(&mut phrase)?;
                KeyCredential::Passphrase(phrase)
            }
            2 if n == 32 => {
                let mut bytes = Zeroizing::new([0u8; 32]);
                read(&mut bytes[..])?;
                KeyCredential::Recovery(Secret::from_bytes(*bytes))
            }
            _ => return Err(malformed()),
        };
        domains.push((id, credential));
    }
    let mut extra = Zeroizing::new([0u8; 1]);
    if pipe.read(&mut extra[..]).map_err(|_| malformed())? != 0 {
        return Err(malformed());
    }
    Ok(KeyFrame {
        pool,
        admin,
        domains,
    })
}
