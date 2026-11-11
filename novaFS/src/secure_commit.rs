//! Journal de publication P7. Huit blocs réservés, un seul écrivain détenant
//! le périphérique et la sous-clé administrative de publication. Chaque bande
//! contient deux attestations dans des blocs disjoints. F1 stabilise le graphe
//! complet avant la production de ces attestations. F2 précède l'acquittement.
use crate::{BLOCK_BYTES, Block, Device, Error, Result, crypto::Secret};

pub type Digest = [u8; 32];
pub const COMMIT_BLOCKS: u64 = 8;
const TAG: usize = BLOCK_BYTES - 32;
const LABEL: &[u8] = b"NovaFS/P7/COMMIT/v1";
const MAGIC: &[u8; 8] = b"NP7COM01";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Head {
    pub generation: u64,
    pub digest: Digest,
    pub parent: Digest,
}

impl Head {
    fn encode(&self, layout: Layout, offset: u64, key: &Secret) -> Block {
        let mut block = [0; BLOCK_BYTES];
        block[..8].copy_from_slice(MAGIC);
        block[8..24].copy_from_slice(&layout.pool);
        block[24..32].copy_from_slice(&layout.base.to_le_bytes());
        block[32..40].copy_from_slice(&offset.to_le_bytes());
        block[40..48].copy_from_slice(&self.generation.to_le_bytes());
        block[48..80].copy_from_slice(&self.digest);
        block[80..112].copy_from_slice(&self.parent);
        let tag = key.mac(&[LABEL, &block[..TAG]]);
        block[TAG..].copy_from_slice(&tag);
        block
    }

    fn decode(block: &Block, layout: Layout, offset: u64, key: &Secret) -> Result<Option<Self>> {
        let tag = block[TAG..].try_into().expect("tag fixe");
        if key.verify_mac(&[LABEL, &block[..TAG]], tag).is_err() {
            // Le remplacement d'une ancienne attestation peut laisser un mélange
            // old/new. Une telle suite d'octets n'autorise aucune publication.
            return Ok(None);
        }
        Self::inspect_public(block, layout.pool, layout.base, offset).map(Some)
    }

    /// Décode uniquement la forme publique d'une attestation dont la présence
    /// a déjà été constatée. Cette opération ne vérifie aucune autorité.
    pub(crate) fn inspect_public(
        block: &Block,
        pool: [u8; 16],
        base: u64,
        offset: u64,
    ) -> Result<Self> {
        let number = |start| u64::from_le_bytes(block[start..start + 8].try_into().unwrap());
        let head = Self {
            generation: number(40),
            digest: block[48..80].try_into().unwrap(),
            parent: block[80..112].try_into().unwrap(),
        };
        if &block[..8] != MAGIC
            || block[8..24] != pool
            || number(24) != base
            || number(32) != offset
            || head.generation == 0
            || head.generation % 4 != offset / 2
            || (head.generation == 1 && head.parent != [0; 32])
            || block[112..TAG].iter().any(|b| *b != 0)
        {
            return Err(Error::Corrupt("attestation P7 hors contexte"));
        }
        Ok(head)
    }

    pub(crate) fn inspect_authenticated(
        block: &Block,
        pool: [u8; 16],
        base: u64,
        offset: u64,
        key: &Secret,
    ) -> Result<Self> {
        Self::decode(block, Layout { base, pool }, offset, key)?.ok_or(Error::Corrupt(
            "MAC invalide sur une copie d'attestation P7",
        ))
    }
}

#[derive(Clone, Copy)]
struct Layout {
    base: u64,
    pool: [u8; 16],
}

impl Layout {
    fn new(device: &impl Device, base: u64, pool: [u8; 16]) -> Result<Self> {
        if pool == [0; 16]
            || base
                .checked_add(COMMIT_BLOCKS)
                .is_none_or(|n| n > device.blocks())
        {
            return Err(Error::Invalid("géométrie du journal P7"));
        }
        Ok(Self { base, pool })
    }

    fn write_copy(self, d: &mut impl Device, h: Head, replica: u64, key: &Secret) -> Result<()> {
        let offset = 2 * (h.generation % 4) + replica;
        d.write(self.base + offset, &h.encode(self, offset, key))
    }

    fn scan<D: Device>(
        self,
        d: &D,
        key: &Secret,
        mut verify: impl FnMut(&D, &Head) -> Result<()>,
    ) -> Result<Selection> {
        let mut records = [None; COMMIT_BLOCKS as usize];
        for (i, entry) in records.iter_mut().enumerate() {
            let block = d.read(self.base + i as u64)?;
            *entry = Head::decode(&block, self, i as u64, key)?;
        }
        for copies in records.chunks_exact(2) {
            if let [Some(a), Some(b)] = copies
                && a.generation == b.generation
                && a != b
            {
                return Err(Error::Corrupt("attestations P7 divergentes"));
            }
        }
        let head = records
            .iter()
            .flatten()
            .max_by_key(|v| v.generation)
            .copied()
            .ok_or(Error::Corrupt("aucune attestation P7 valide"))?;
        let parent = if head.generation == 1 {
            None
        } else {
            let previous = records
                .iter()
                .flatten()
                .find(|p| p.generation == head.generation - 1)
                .ok_or(Error::Corrupt("prédécesseur P7 inaccessible"))?;
            if head.parent != previous.digest {
                return Err(Error::Corrupt("filiation des attestations P7"));
            }
            verify(d, previous)?;
            Some(*previous)
        };
        // Toute panne sur un catalogue sélectionné ou son graphe ferme le
        // montage. Une ancienne attestation valide ne fournit aucun repli.
        verify(d, &head)?;
        Ok(Selection {
            records,
            head,
            parent,
        })
    }
}

struct Selection {
    records: [Option<Head>; COMMIT_BLOCKS as usize],
    head: Head,
    parent: Option<Head>,
}

#[derive(PartialEq)]
enum Mode {
    Inspection,
    Writable,
    Interrupted,
}

pub struct CommitLog {
    layout: Layout,
    head: Head,
    key_check: Digest,
    mode: Mode,
}

impl CommitLog {
    fn build(layout: Layout, head: Head, key: &Secret, mode: Mode) -> Self {
        let key_check = key.mac(&[b"NovaFS/P7/mount", &layout.pool, &layout.base.to_le_bytes()]);
        Self {
            layout,
            head,
            key_check,
            mode,
        }
    }

    fn check_key(&self, key: &Secret) -> Result<()> {
        key.verify_mac(
            &[
                b"NovaFS/P7/mount",
                &self.layout.pool,
                &self.layout.base.to_le_bytes(),
            ],
            &self.key_check,
        )
    }

    /// L'appelant réserve les huit blocs et crée le premier catalogue ainsi que
    /// ses dépendances sur une image neuve. La préparation reçoit la génération.
    pub fn format<D: Device>(
        d: &mut D,
        base: u64,
        pool: [u8; 16],
        key: &Secret,
        prepare: impl FnOnce(&mut D, u64) -> Result<Digest>,
    ) -> Result<Self> {
        let layout = Layout::new(d, base, pool)?;
        for i in 0..COMMIT_BLOCKS {
            if d.read(base + i)?.iter().any(|v| *v != 0) {
                return Err(Error::Exists);
            }
        }
        let head = Self::install(d, layout, key, 1, [0; 32], prepare)?;
        Ok(Self::build(layout, head, key, Mode::Writable))
    }

    /// Inspection strictement en lecture seule. Le rappel vérifie le catalogue
    /// dont l'empreinte et la génération sont authentifiées et ses dépendances.
    pub fn open<D: Device>(
        d: &D,
        base: u64,
        pool: [u8; 16],
        key: &Secret,
        verify: impl FnMut(&D, &Head) -> Result<()>,
    ) -> Result<Self> {
        let layout = Layout::new(d, base, pool)?;
        let selection = layout.scan(d, key, verify)?;
        Ok(Self::build(layout, selection.head, key, Mode::Inspection))
    }

    pub fn head(&self) -> Head {
        self.head
    }

    /// Sur le même périphérique exclusif, après drainage des anciennes E/S,
    /// stabilise la paire inspectée et complète ses attestations manquantes.
    /// L'échec de cet appel ou d'une publication impose une nouvelle inspection.
    pub fn resume<D: Device>(
        &mut self,
        d: &mut D,
        key: &Secret,
        verify: impl FnMut(&D, &Head) -> Result<()>,
    ) -> Result<()> {
        self.check_key(key)?;
        match self.mode {
            Mode::Writable => return Ok(()),
            Mode::Interrupted => return Err(Error::UnknownOutcome),
            Mode::Inspection => {}
        }
        self.mode = Mode::Interrupted;
        d.flush()?;
        let selection = self.layout.scan(d, key, verify)?;
        if self.head != selection.head {
            return Err(Error::Corrupt("tête P7 modifiée pendant la reprise"));
        }
        let mut repaired = false;
        for head in [Some(selection.head), selection.parent]
            .into_iter()
            .flatten()
        {
            for replica in 0..2 {
                let offset = 2 * (head.generation % 4) + replica;
                if selection.records[offset as usize] != Some(head) {
                    self.layout.write_copy(d, head, replica, key)?;
                    repaired = true;
                }
            }
        }
        if repaired {
            d.flush()?;
        }
        self.mode = Mode::Writable;
        Ok(())
    }

    /// La préparation écrit sur des blocs hors des deux graphes protégés,
    /// y compris le catalogue candidat. Le journal et les deux catalogues
    /// protégés sont réservés à leur propriétaire. L'appelant conserve le même
    /// périphérique sous son autorité exclusive d'un appel au suivant.
    pub fn publish<D: Device>(
        &mut self,
        d: &mut D,
        key: &Secret,
        prepare: impl FnOnce(&mut D, u64) -> Result<Digest>,
    ) -> Result<Head> {
        match self.mode {
            Mode::Inspection => return Err(Error::ReadOnly),
            Mode::Interrupted => return Err(Error::UnknownOutcome),
            Mode::Writable => {}
        }
        self.check_key(key)?;
        let generation = self
            .head
            .generation
            .checked_add(1)
            .ok_or(Error::Limit("génération P7"))?;
        self.mode = Mode::Interrupted;
        let head = Self::install(d, self.layout, key, generation, self.head.digest, prepare)?;
        self.head = head;
        self.mode = Mode::Writable;
        Ok(head)
    }

    fn install<D: Device>(
        d: &mut D,
        layout: Layout,
        key: &Secret,
        generation: u64,
        parent: Digest,
        prepare: impl FnOnce(&mut D, u64) -> Result<Digest>,
    ) -> Result<Head> {
        let digest = prepare(d, generation)?;
        d.flush()?;
        let head = Head {
            generation,
            digest,
            parent,
        };
        // Même un refus de type NoSpace peut suivre une écriture partielle
        // ou acquittée par le support. Préserver la cause et l'issue inconnue.
        let indeterminate = |phase, cause| Error::Publication {
            phase,
            cause: Box::new(cause),
        };
        layout
            .write_copy(d, head, 0, key)
            .map_err(|e| indeterminate("première attestation P7", e))?;
        layout
            .write_copy(d, head, 1, key)
            .map_err(|e| indeterminate("seconde attestation P7", e))?;
        d.flush()
            .map_err(|e| indeterminate("F2 des attestations P7", e))?;
        Ok(head)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::{Event, Persistence, SimDevice, SimFault};
    use sha2::{Digest as _, Sha256};

    const BASE: u64 = 5;
    const POOL: [u8; 16] = [23; 16];
    const CATALOGS: u64 = BASE + COMMIT_BLOCKS;

    fn prepare(d: &mut SimDevice, generation: u64, value: u8) -> Result<Digest> {
        let mut block = [value; BLOCK_BYTES];
        block[..8].copy_from_slice(&generation.to_le_bytes());
        d.write(CATALOGS + generation % 4, &block)?;
        Ok(Sha256::digest(block).into())
    }

    fn verify(d: &SimDevice, h: &Head) -> Result<()> {
        let block = d.read(CATALOGS + h.generation % 4)?;
        let digest: Digest = Sha256::digest(block).into();
        if digest != h.digest || block[..8] != h.generation.to_le_bytes() {
            return Err(Error::Corrupt("catalogue de test P7"));
        }
        Ok(())
    }

    fn formatted(sector: u32, key: &Secret) -> Result<(SimDevice, CommitLog)> {
        let mut d = SimDevice::new(32, sector)?;
        let mut log = CommitLog::format(&mut d, BASE, POOL, key, |d, g| prepare(d, g, 1))?;
        for value in 2..=5 {
            log.publish(&mut d, key, |d, g| prepare(d, g, value))?;
        }
        Ok((d, log))
    }

    #[test]
    fn chaque_frontiere_et_reprise_decriture() -> Result<()> {
        let key = Secret::from_bytes([69; 32]);
        for sector in [512, 4096] {
            let (mut d, mut log) = formatted(sector, &key)?;
            let initial = d.control().snapshot();
            let old = log.head();
            let ctl = d.control();
            ctl.trace(None, None);
            let new = log.publish(&mut d, &key, |d, g| prepare(d, g, 77))?;
            let trace = ctl.events();
            assert_eq!(
                trace
                    .iter()
                    .filter(|e| matches!(e, Event::FlushCompleted(_)))
                    .count(),
                2
            );
            let f1 = trace
                .iter()
                .position(|e| *e == Event::FlushCompleted(1))
                .unwrap();
            let f2 = trace
                .iter()
                .position(|e| *e == Event::FlushEffect(2))
                .unwrap();
            for cut in 0..=trace.len() {
                for persistence in [
                    Persistence::None,
                    Persistence::Newest(1),
                    Persistence::Newest(2),
                    Persistence::Prefix {
                        block: BASE + 4,
                        bytes: BLOCK_BYTES - 9,
                    },
                    Persistence::Sectors {
                        block: BASE + 5,
                        mask: 1,
                    },
                ] {
                    let mut d = initial.device();
                    let mut log = CommitLog::open(&d, BASE, POOL, &key, verify)?;
                    log.resume(&mut d, &key, verify)?;
                    let ctl = d.control();
                    ctl.trace(Some(cut), None);
                    let outcome = log.publish(&mut d, &key, |d, g| prepare(d, g, 77));
                    assert_eq!(outcome.is_ok(), cut == trace.len());
                    if outcome.is_err() {
                        assert!(matches!(
                            log.publish(&mut d, &key, |_, _| unreachable!()),
                            Err(Error::UnknownOutcome)
                        ));
                    }
                    ctl.power_cut(persistence)?;
                    let mut mounted = CommitLog::open(&d, BASE, POOL, &key, verify)?;
                    let found = mounted.head();
                    assert!(found == old || found == new);
                    if cut <= f1 {
                        assert_eq!(found, old);
                    }
                    if cut >= f2 {
                        assert_eq!(found, new);
                    }
                    assert!(matches!(
                        mounted.publish(&mut d, &key, |_, _| unreachable!()),
                        Err(Error::ReadOnly)
                    ));
                    mounted.resume(&mut d, &key, verify)?;
                    let next = mounted.publish(&mut d, &key, |d, g| prepare(d, g, 91))?;
                    assert_eq!(next.parent, found.digest);
                    assert_eq!(CommitLog::open(&d, BASE, POOL, &key, verify)?.head(), next);
                }
            }
        }
        Ok(())
    }

    #[test]
    fn certificats_doubles_dechirures_et_rejeu_isole() -> Result<()> {
        let key = Secret::from_bytes([42; 32]);
        for sector in [512, 4096] {
            let (mut d, mut log) = formatted(sector, &key)?;
            let before = [d.read(BASE + 4)?, d.read(BASE + 5)?];
            let old = log.head();
            let head = log.publish(&mut d, &key, |d, g| prepare(d, g, 68))?;
            let after = [d.read(BASE + 4)?, d.read(BASE + 5)?];
            let snap = d.control().snapshot();
            for m in [0x00u8, 0x01, 0x55, 0xaa, 0xfe, 0xff] {
                for n in [0x00u8, 0x01, 0x55, 0xaa, 0xfe, 0xff] {
                    let mut partial = snap.device();
                    let ctl = partial.control();
                    for (i, mask) in [m, n].into_iter().enumerate() {
                        ctl.replace_stable(
                            BASE + 4 + i as u64,
                            core::array::from_fn(|b| (before[i][b] & !mask) | (after[i][b] & mask)),
                        )?;
                    }
                    let mounted = CommitLog::open(&partial, BASE, POOL, &key, verify)?;
                    let committed = m == 255 || n == 255;
                    assert_eq!(mounted.head(), if committed { head } else { old });
                    let no_repair = partial.control().snapshot().bytes()?;
                    let mut recheck = CommitLog::open(&partial, BASE, POOL, &key, verify)?;
                    assert_eq!(partial.control().snapshot().bytes()?, no_repair);
                    recheck.resume(&mut partial, &key, verify)?;
                    if committed {
                        assert_eq!(partial.read(BASE + 4)?, after[0]);
                        assert_eq!(partial.read(BASE + 5)?, after[1]);
                    }
                }
            }
            for offset in [0u64, 1] {
                let broken = snap.device();
                broken
                    .control()
                    .replace_stable(BASE + 4 + offset, [0xff; BLOCK_BYTES])?;
                assert_eq!(
                    CommitLog::open(&broken, BASE, POOL, &key, verify)?.head(),
                    head
                );
            }
            // Un bloc engagé dont le catalogue est détruit ne permet aucun
            // repli sur la génération précédente, même si celle-ci est valide.
            let lost = snap.device();
            lost.control()
                .replace_stable(CATALOGS + head.generation % 4, [0; BLOCK_BYTES])?;
            assert!(CommitLog::open(&lost, BASE, POOL, &key, verify).is_err());
        }
        Ok(())
    }

    #[test]
    fn substitution_de_fork_contexte_generation_et_cle() -> Result<()> {
        let key = Secret::from_bytes([9; 32]);
        let bad = Secret::from_bytes([8; 32]);
        for sector in [512, 4096] {
            let (mut left, mut log) = formatted(sector, &key)?;
            let mut right = left.control().snapshot().device();
            let mut sibling = CommitLog::open(&right, BASE, POOL, &key, verify)?;
            sibling.resume(&mut right, &key, verify)?;
            let good = log.publish(&mut left, &key, |d, g| prepare(d, g, 77))?;
            let fork = sibling.publish(&mut right, &key, |d, g| prepare(d, g, 88))?;
            assert_eq!(good.generation, fork.generation);
            assert_ne!(good.digest, fork.digest);
            let snapshot = left.control().snapshot();
            left.control()
                .replace_stable(BASE + 5, right.read(BASE + 5)?)?;
            assert!(CommitLog::open(&left, BASE, POOL, &key, verify).is_err());
            left.control()
                .replace_stable(BASE + 4, right.read(BASE + 4)?)?;
            assert!(CommitLog::open(&left, BASE, POOL, &key, verify).is_err());

            let mut d = snapshot.device();
            assert!(CommitLog::open(&d, BASE, POOL, &bad, verify).is_err());
            assert!(CommitLog::open(&d, BASE, [57; 16], &key, verify).is_err());
            assert!(CommitLog::open(&d, u64::MAX, POOL, &key, verify).is_err());
            let mut ro = CommitLog::open(&d, BASE, POOL, &key, verify)?;
            assert!(ro.resume(&mut d, &bad, verify).is_err());
            ro.resume(&mut d, &key, verify)?;
            assert!(ro.publish(&mut d, &bad, |_, _| unreachable!()).is_err());
            assert!(matches!(
                CommitLog::format(&mut d, BASE, POOL, &key, |_, _| unreachable!()),
                Err(Error::Exists)
            ));
            let layout = Layout::new(&d, BASE, POOL)?;
            for changed in [
                Head {
                    generation: 0,
                    ..good
                },
                Head {
                    generation: good.generation + 1,
                    ..good
                },
                Head {
                    generation: good.generation + 4,
                    ..good
                },
                Head {
                    parent: [66; 32],
                    ..good
                },
            ] {
                let d = snapshot.device();
                for offset in [4, 5] {
                    d.control()
                        .replace_stable(BASE + offset, changed.encode(layout, offset, &key))?;
                }
                assert!(CommitLog::open(&d, BASE, POOL, &key, verify).is_err());
            }
            let d = snapshot.device();
            d.control().replace_stable(BASE + 5, d.read(BASE + 4)?)?;
            assert!(CommitLog::open(&d, BASE, POOL, &key, verify).is_err());
            let raw = snapshot.bytes()?;
            assert!(SimDevice::from_bytes(&raw[..raw.len() - 1], sector).is_err());
            let truncated =
                SimDevice::from_bytes(&raw[..(BASE as usize + 7) * BLOCK_BYTES], sector)?;
            assert!(CommitLog::open(&truncated, BASE, POOL, &key, verify).is_err());
        }
        Ok(())
    }

    #[test]
    fn penurie_signalee_apres_effet_du_peripherique() -> Result<()> {
        struct ApresEffet {
            disk: SimDevice,
            rank: u8,
            stop: u8,
        }
        impl ApresEffet {
            fn acknowledge(&mut self) -> Result<()> {
                self.rank += 1;
                if self.rank == self.stop {
                    Err(Error::NoSpace)
                } else {
                    Ok(())
                }
            }
        }
        impl Device for ApresEffet {
            fn blocks(&self) -> u64 {
                self.disk.blocks()
            }
            fn read(&self, b: u64) -> Result<Block> {
                self.disk.read(b)
            }
            fn write(&mut self, b: u64, p: &Block) -> Result<()> {
                self.disk.write(b, p)?;
                self.acknowledge()
            }
            fn flush(&mut self) -> Result<()> {
                self.disk.flush()?;
                self.acknowledge()
            }
        }
        let key = Secret::from_bytes([27; 32]);
        for sector in [512, 4096] {
            let (d, _) = formatted(sector, &key)?;
            for stop in 1..=5 {
                let mut disk = d.control().snapshot().device();
                let mut log = CommitLog::open(&disk, BASE, POOL, &key, verify)?;
                log.resume(&mut disk, &key, verify)?;
                let mut altered = ApresEffet {
                    disk,
                    rank: 0,
                    stop,
                };
                let error = log
                    .publish(&mut altered, &key, |d, g| {
                        let mut p = [67; BLOCK_BYTES];
                        p[..8].copy_from_slice(&g.to_le_bytes());
                        d.write(CATALOGS + g % 4, &p)?;
                        Ok(Sha256::digest(p).into())
                    })
                    .unwrap_err();
                if stop >= 3 {
                    assert!(
                        matches!(error,Error::Publication{cause,..} if matches!(*cause,Error::NoSpace))
                    );
                } else {
                    assert!(matches!(error, Error::NoSpace));
                }
                assert!(matches!(
                    log.publish(&mut altered, &key, |_, _| unreachable!()),
                    Err(Error::UnknownOutcome)
                ));
                altered.disk.control().power_cut(Persistence::Newest(2))?;
                let restored = CommitLog::open(&altered.disk, BASE, POOL, &key, verify)?;
                assert_eq!(restored.head().generation, if stop >= 3 { 6 } else { 5 });
            }
        }
        Ok(())
    }

    #[test]
    fn erreurs_de_lecture_ecriture_et_flush() -> Result<()> {
        let key = Secret::from_bytes([27; 32]);
        for sector in [512, 4096] {
            let (d, _) = formatted(sector, &key)?;
            for i in 0..COMMIT_BLOCKS {
                d.control().inject(SimFault::Read { block: BASE + i })?;
                assert!(matches!(
                    CommitLog::open(&d, BASE, POOL, &key, verify),
                    Err(Error::Io(_))
                ));
            }
            for fault in [
                SimFault::Write {
                    block: CATALOGS + 2,
                    prefix: 35,
                },
                SimFault::Write {
                    block: BASE + 4,
                    prefix: BLOCK_BYTES - 11,
                },
                SimFault::Write {
                    block: BASE + 5,
                    prefix: 16,
                },
                SimFault::Flush {
                    ordinal: 1,
                    persist_newest: 1,
                },
                SimFault::Flush {
                    ordinal: 2,
                    persist_newest: 0,
                },
                SimFault::Flush {
                    ordinal: 2,
                    persist_newest: 1,
                },
            ] {
                let mut copy = d.control().snapshot().device();
                let mut log = CommitLog::open(&copy, BASE, POOL, &key, verify)?;
                log.resume(&mut copy, &key, verify)?;
                copy.control().inject(fault)?;
                let error = log
                    .publish(&mut copy, &key, |d, g| prepare(d, g, 63))
                    .unwrap_err();
                match fault {
                    SimFault::Write { block, .. } if block == CATALOGS + 2 => {
                        assert!(matches!(error, Error::Io(_)))
                    }
                    SimFault::Flush { ordinal: 1, .. } => assert!(matches!(error, Error::Io(_))),
                    _ => assert!(
                        matches!(error,Error::Publication{cause,..} if matches!(*cause,Error::Io(_)))
                    ),
                }
                assert!(matches!(
                    log.resume(&mut copy, &key, verify),
                    Err(Error::UnknownOutcome)
                ));
                copy.control().power_cut(Persistence::None)?;
                let recovered = CommitLog::open(&copy, BASE, POOL, &key, verify)?;
                let expected = if matches!(
                    fault,
                    SimFault::Flush {
                        ordinal: 2,
                        persist_newest: 1
                    }
                ) {
                    6
                } else {
                    5
                };
                assert_eq!(recovered.head().generation, expected);
            }
        }
        Ok(())
    }
}
