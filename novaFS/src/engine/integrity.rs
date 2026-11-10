//! Diagnostic exhaustif des blocs atteignables d'un LV. Les deux parcours
//! indépendants continuent même si l'autre ancre est illisible. Le support
//! doit rester immuable pendant l'analyse ; aucun descripteur d'écriture
//! n'est nécessaire et le vérificateur n'appelle jamais flush.
use super::{DiskView, Head, check_generation, geometry, inspect_heads};
use crate::{Device, Error, Ids, codec::Anchor};

#[derive(Debug)]
pub struct CheckIssue {
    pub stage: &'static str,
    /// Présent uniquement lorsqu'une structure ou une E/S fautive est localisée.
    pub block: Option<u64>,
    pub error: Error,
}

#[derive(Debug)]
pub struct SlotReport {
    /// Enveloppe décodée et protégée par son checksum. Son contenu peut être
    /// incompatible avec les identités externes ou ses dépendances sur disque.
    pub anchor: Option<Anchor>,
    /// Première erreur dans cette génération. None exige une validation intégrale.
    pub issue: Option<CheckIssue>,
}

#[derive(Debug)]
pub enum PairStatus {
    Verified {
        slot: usize,
    },
    /// Au moins un slot n'a pu être validé indépendamment.
    Unavailable,
    /// Les deux vues sont lisibles mais leur cohérence commune n'a pas été établie.
    Rejected(CheckIssue),
}

#[derive(Debug)]
pub struct IntegrityReport {
    pub slots: [SlotReport; 2],
    pub pair: PairStatus,
}

pub fn check_image<D: Device>(dev: &D, ids: Ids) -> IntegrityReport {
    check_image_with_bitmap_budget(dev, ids, usize::MAX)
}

/// Le plafond porte sur les deux cartes de propriété de blocs, soit
/// 2 × ceil(blocs / 64) mots u64. Toute vérification est refusée si ce crédit
/// est insuffisant. Les nœuds décodés et intervalles ont leurs propres bornes.
pub fn check_image_with_bitmap_budget<D: Device>(
    dev: &D,
    ids: Ids,
    bitmap_bytes: usize,
) -> IntegrityReport {
    let (first, h0) = scan_slot(dev, ids, 0, bitmap_bytes);
    let (second, h1) = scan_slot(dev, ids, 1, bitmap_bytes);
    let pair = match (h0, h1) {
        (Some(a), Some(b)) => {
            // Les deux vues ont déjà relu tous leurs payloads. Cette passe
            // applique les mêmes règles intergénérationnelles que le montage.
            match inspect_heads(dev, ids, a, b, false) {
                Ok((head, _)) => PairStatus::Verified {
                    slot: head.slot as usize,
                },
                Err(error) => PairStatus::Rejected(CheckIssue {
                    stage: "paire",
                    block: None,
                    error,
                }),
            }
        }
        _ => PairStatus::Unavailable,
    };
    IntegrityReport {
        slots: [first, second],
        pair,
    }
}

fn scan_slot<D: Device>(
    dev: &D,
    ids: Ids,
    slot: u64,
    bitmap_bytes: usize,
) -> (SlotReport, Option<Head>) {
    let mut report = SlotReport {
        anchor: None,
        issue: None,
    };
    let result = (|| {
        geometry(dev, ids).map_err(|error| CheckIssue {
            stage: "geometrie",
            block: None,
            error,
        })?;
        // geometry borne la capacité à 2^21 blocs sur les deux cibles.
        if bitmap_bytes < dev.blocks().div_ceil(64) as usize * 2 * size_of::<u64>() {
            return Err(CheckIssue {
                stage: "memoire",
                block: None,
                error: Error::NoMemory,
            });
        }
        let at_anchor = |error| CheckIssue {
            stage: "ancre",
            block: Some(slot),
            error,
        };
        let a = Anchor::decode(&dev.read(slot).map_err(at_anchor)?, slot).map_err(at_anchor)?;
        report.anchor = Some(a);
        if a.ids != ids || a.blocks != dev.blocks() {
            return Err(at_anchor(Error::Corrupt("identité ou géométrie du LV")));
        }
        let h = Head::from_anchor(dev, ids, slot, a).map_err(|error| CheckIssue {
            stage: "manifeste",
            block: Some(a.manifest.block),
            error,
        })?;
        let view = DiskView::new(dev, ids, &h.m);
        check_generation(&view, &h, None, true).map_err(|error| CheckIssue {
            stage: "generation",
            block: view.failed_block.get(),
            error,
        })?;
        Ok(h)
    })();
    match result {
        Ok(h) => (report, Some(h)),
        Err(issue) => {
            report.issue = Some(issue);
            (report, None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Engine, Mode, Result, device::SimDevice};

    const IDS: Ids = Ids {
        fs: [9; 16],
        lv: [10; 16],
    };

    #[test]
    fn p2_credit_insuffisant_ne_certifie_aucune_vue() -> Result<()> {
        let dev = Engine::format(SimDevice::new(257, 512)?, IDS)?.unmount()?;
        // 257 blocs requièrent cinq mots dans chacun des deux bitmaps.
        let denied = check_image_with_bitmap_budget(&dev, IDS, 79);
        assert!(matches!(denied.pair, PairStatus::Unavailable));
        for slot in denied.slots {
            assert!(matches!(
                slot.issue,
                Some(CheckIssue {
                    error: Error::NoMemory,
                    ..
                })
            ));
        }
        let exact = check_image_with_bitmap_budget(&dev, IDS, 80);
        assert!(matches!(exact.pair, PairStatus::Verified { slot: 0 }));
        Ok(())
    }

    #[test]
    fn p2_bloc_libre_declare_retire_du_predecesseur() -> Result<()> {
        let mut engine = Engine::format(SimDevice::new(320, 4096)?, IDS)?;
        engine.transact(false, |tx| {
            // Ce bloc était déjà libre. Le faux registre prétend qu'il était
            // encore détenu à la génération précédente. Les deux partitions
            // locales restent exhaustives et toutes les sommes sont calculées.
            let b = tx.space.take()?;
            tx.space.release(b, tx.m.previous, tx.m.blocks)?;
            tx.m.next_oid += 1;
            Ok(())
        })?;
        let dev = engine.into_device();
        let report = check_image(&dev, IDS);
        assert!(report.slots.iter().all(|s| s.issue.is_none()));
        assert!(matches!(
            report.pair,
            PairStatus::Rejected(CheckIssue {
                error: Error::Corrupt("âge du bloc libéré"),
                ..
            })
        ));
        assert!(matches!(
            Engine::mount(dev, IDS, Mode::ReadWrite),
            Err(Error::Corrupt(_))
        ));
        Ok(())
    }
}
