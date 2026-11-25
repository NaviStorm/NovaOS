//! Observation privée de la liaison physique d'un dossier EXEC scellé.
use novafs::View;
use crate::{Fault, Hash, Id, Result, recuperation_view::seal_digest};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScopedFile {
    pub directory: u64,
    pub file: u64,
    pub seal: Hash,
}

pub struct Probe<'a>(&'a [u8; 128]);

impl<'a> Probe<'a> {
    pub const TAG: &'static [u8; 8] = b"NXEXFS01";
    pub const SELECT: &'static [u8; 8] = b"NXEXFSD1";

    pub fn supports(frame: &[u8; 128]) -> bool {
        &frame[..8] == Self::TAG || &frame[..8] == Self::SELECT
    }

    pub fn parse(frame: &'a [u8; 128]) -> Result<Self> {
        let probe = Self(frame);
        if !Self::supports(frame) || frame[120..] != [0; 8]
            || frame[8..24] == [0; 16]
            || probe.volume().contains(&[0; 16])
            || frame[56..72] == [0; 16]
            || probe.directory() == probe.file()
            || !(1..1 << 63).contains(&probe.directory())
            || if &frame[..8] == Self::SELECT {
                probe.file() != 0
            } else {
                !(1..1 << 63).contains(&probe.file())
            }
            || frame[88..120] == [0; 32]
        {
            return Err(Fault::BadInput);
        }
        Ok(probe)
    }

    pub fn volume(&self) -> [Id; 2] {
        [self.0[24..40].try_into().unwrap(), self.0[40..56].try_into().unwrap()]
    }

    pub fn directory(&self) -> u64 {
        u64::from_le_bytes(self.0[72..80].try_into().unwrap())
    }

    pub fn file(&self) -> u64 {
        u64::from_le_bytes(self.0[80..88].try_into().unwrap())
    }

    pub fn verify_context(&self, volume: [Id; 2], instance: [u8; 32]) -> Result<()> {
        if self.volume() != volume || self.0[88..120] != instance {
            return Err(Fault::Denied);
        }
        Ok(())
    }

    /// Observation sans lecture des données utilisateur et sans octroi.
    pub fn inspect<D: novafs::Device>(&self, fs: &novafs::Engine<D>,
        instance: [u8; 32], seal: &[u8; 40]) -> Result<[u8; 128]> {
        let ids = fs.ids();
        self.verify_context([ids.fs, ids.lv], instance)?;
        let reply = self.reply(seal)?;
        let id = u64::from_le_bytes(seal[32..40].try_into().unwrap());
        let directory = fs.stat_id(self.directory())?;
        let file = fs.stat_id(id)?;
        if directory.kind != novafs::Kind::Directory || file.kind != novafs::Kind::File
            || fs.parent_name(id)?.0 != self.directory()
        {
            return Err(Fault::Denied);
        }
        Ok(reply)
    }

    fn seal_matches(&self, seal: &[u8]) -> bool {
        if seal.len() != 40 || &seal[..8] != b"NXScope1" || seal[8..32] != self.0[56..80] {
            return false;
        }
        let file = u64::from_le_bytes(seal[32..40].try_into().unwrap());
        file != self.directory()
            && (1..1 << 63).contains(&file)
            && (&self.0[..8] == Self::SELECT || file == self.file())
    }

    pub fn reply(&self, seal: &[u8; 40]) -> Result<[u8; 128]> {
        if !self.seal_matches(seal) {
            return Err(Fault::Denied);
        }
        let mut reply = [0; 128];
        reply[8..16].copy_from_slice(b"NXEXFSR1");
        reply[16..48].copy_from_slice(&seal_digest(self.0));
        reply[48..80].copy_from_slice(&self.0[88..120]);
        reply[80..120].copy_from_slice(seal);
        Ok(reply)
    }

    pub fn verify(&self, reply: &[u8; 128]) -> Result<Hash> {
        if &self.0[..8] != Self::TAG {
            return Err(Fault::Denied);
        }
        self.verified_seal(reply)
    }

    /// Une découverte ne devient exploitable qu'avec le sceau de la vue
    /// authentifiée du registre. Le montage seul ne décide aucune permission.
    pub fn verify_selection(&self, reply: &[u8; 128], expected: Hash) -> Result<ScopedFile> {
        if &self.0[..8] != Self::SELECT || self.verified_seal(reply)? != expected {
            return Err(Fault::Denied);
        }
        Ok(ScopedFile {
            directory: self.directory(),
            file: u64::from_le_bytes(reply[112..120].try_into().unwrap()),
            seal: expected,
        })
    }

    fn verified_seal(&self, reply: &[u8; 128]) -> Result<Hash> {
        if reply[..8] != [0; 8] || &reply[8..16] != b"NXEXFSR1"
            || reply[16..48] != seal_digest(self.0)
            || reply[48..80] != self.0[88..120]
            || !self.seal_matches(&reply[80..120]) || reply[120..] != [0; 8]
        {
            return Err(Fault::Denied);
        }
        Ok(seal_digest(&reply[80..120]))
    }
}
