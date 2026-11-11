//! Assemble uniquement les nouvelles pages marquées données par le moteur FS.
//! Le singleton conserve une copie de 4 Kio pour pouvoir revenir au format brut.
use super::{Block, Device, Graph, PageRef, Result, graph};
use crate::compression::{Packer, Push};
use std::collections::BTreeMap;

pub(super) struct Group {
    packer: Packer,
    first: Block,
}

impl Drop for Group {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.first);
    }
}

impl Default for Group {
    fn default() -> Self {
        Self {
            packer: Packer::default(),
            first: [0; 4096],
        }
    }
}

impl Group {
    pub(super) fn add(
        &mut self,
        graph: &mut Graph,
        dev: &mut impl Device,
        refs: &mut BTreeMap<u64, PageRef>,
        lba: u64,
        data: &Block,
    ) -> Result<()> {
        let slot = u16::try_from(lba).map_err(|_| crate::Error::Invalid("LBA du pack"))?;
        let mut result = self.packer.push(slot, data)?;
        if matches!(result, Push::Full) {
            self.publish(graph, dev, refs)?;
            result = self.packer.push(slot, data)?;
        }
        match result {
            Push::Added => {
                if self.packer.lbas().count() == 1 {
                    self.first = *data;
                }
            }
            Push::Raw => {
                refs.insert(lba, graph::store_data(graph, dev, lba, data)?);
            }
            Push::Full => return Err(crate::Error::Invalid("pack vide refusé")),
        }
        Ok(())
    }

    pub(super) fn publish(
        &mut self,
        graph: &mut Graph,
        dev: &mut impl Device,
        refs: &mut BTreeMap<u64, PageRef>,
    ) -> Result<()> {
        let pack = std::mem::take(&mut self.packer);
        let mut slots = [0u16; 16];
        let mut count = 0;
        for lba in pack.lbas() {
            slots[count] = lba;
            count += 1;
        }
        if count == 0 {
            return Ok(());
        }
        let reference = match pack.finish() {
            Some(unit) => graph::store_unit(graph, dev, slots[0], &unit)?,
            None => graph::store_data(graph, dev, u64::from(slots[0]), &self.first)?,
        };
        for lba in &slots[..count] {
            refs.insert(u64::from(*lba), reference);
        }
        Ok(())
    }
}
