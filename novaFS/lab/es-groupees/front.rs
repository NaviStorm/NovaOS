//! Clients du scénario hôte. La capacité à usage unique (socket héritée) désigne
//! une révision et un nom déjà sélectionnés. Aucun chemin ne vient de l'éditeur.
use novafs::{Error, Result};
use std::io::{Read, Write};
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::process::{Child, Command, Stdio};

const DEMANDE: [u8; 3] = [1, 1, b'X'];
fn fd(s: UnixStream) -> Stdio {
    Stdio::from(OwnedFd::from(s))
}
fn ligne(s: &mut UnixStream) -> Result<[u8; 3]> {
    let mut q = [0; 3];
    s.read_exact(&mut q)?;
    Ok(q)
}

pub fn client(editeur: bool) -> Result<()> {
    let mut aval = UnixStream::from(std::io::stdout().as_fd().try_clone_to_owned()?);
    let mut amont = UnixStream::from(std::io::stdin().as_fd().try_clone_to_owned()?);
    let q = ligne(&mut amont)?;
    if q != DEMANDE {
        return Err(Error::Invalid("travail non délégué"));
    }
    aval.write_all(&q)?;
    let a = ligne(&mut aval)?;
    if a != [1, 1, 0] {
        return Err(Error::Unavailable);
    }
    amont.write_all(&a)?;
    // Ce processus n'accepte aucun deuxième travail pour ce descripteur.
    if editeur {
        aval.shutdown(std::net::Shutdown::Both)?;
    }
    Ok(())
}

pub struct Session {
    entree: UnixStream,
    moteur: UnixStream,
    vi: Child,
    courtier: Child,
}
impl Session {
    pub fn nouvelle() -> Result<Self> {
        let (entree, debut) = UnixStream::pair()?;
        let (v, n) = UnixStream::pair()?;
        let (service, moteur) = UnixStream::pair()?;
        let exe = std::env::current_exe()?;
        let vi = Command::new(&exe)
            .arg("--editeur")
            .stdin(fd(debut))
            .stdout(fd(v))
            .spawn()?;
        let courtier = Command::new(exe)
            .arg("--courtier")
            .stdin(fd(n))
            .stdout(fd(service))
            .spawn()?;
        Ok(Self {
            entree,
            moteur,
            vi,
            courtier,
        })
    }
    pub fn lancer(&mut self) -> Result<()> {
        self.entree.write_all(&DEMANDE)?;
        if ligne(&mut self.moteur)? != DEMANDE {
            return Err(Error::Invalid("travail prototypé"));
        }
        Ok(())
    }
    pub fn acquitter(&mut self) -> Result<()> {
        self.moteur.write_all(&[1, 1, 0])?;
        if ligne(&mut self.entree)? != [1, 1, 0] {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.entree.shutdown(std::net::Shutdown::Both);
        let _ = self.moteur.shutdown(std::net::Shutdown::Both);
        let _ = self.vi.wait();
        let _ = self.courtier.wait();
    }
}
