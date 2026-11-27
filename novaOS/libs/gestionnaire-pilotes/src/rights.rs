use crate::{Error, Result, catalogue::Plan, lifecycle::Lease};
use alloc::vec::Vec;
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum Right {
    Class = 1,
    DmaRead = 2,
    DmaWrite = 4,
    Mmio = 8,
    Irq = 16,
}
/// Image compacte des seuls rôles autorisés par le plan.
#[derive(Default)]
pub(crate) struct Grants(Vec<u8>);
impl Grants {
    pub fn from_plan(p: &Plan) -> Result<Self> {
        let h = p.hardware();
        let mut out = Vec::new();
        let n = p
            .capabilities()
            .iter()
            .chain(&h.dma)
            .chain(&h.mmio)
            .chain(&h.irq)
            .map(|s| s.len() + 2)
            .sum();
        out.try_reserve_exact(n).map_err(|_| Error::Memory)?;
        let mut add = |s: &str, rights: u8| {
            out.push(rights);
            out.push(s.len() as u8);
            out.extend_from_slice(s.as_bytes());
        };
        for s in p.capabilities() {
            add(s, Right::Class as u8);
        }
        for s in &h.dma {
            let (role, mode) = s.split_once(':').ok_or(Error::Format)?;
            let rights = match mode {
                "r" => 2,
                "w" => 4,
                "rw" => 6,
                _ => return Err(Error::Format),
            };
            add(role, rights)
        }
        for s in &h.mmio {
            add(s, Right::Mmio as u8)
        }
        for s in &h.irq {
            add(s, Right::Irq as u8)
        }
        Ok(Self(out))
    }
    pub fn offset(&self, right: Right, name: &str) -> Result<usize> {
        let mut i = 0;
        while i < self.0.len() {
            let end = i + 2 + usize::from(self.0[i + 1]);
            if self.0[i] & right as u8 != 0 && &self.0[i + 2..end] == name.as_bytes() {
                return Ok(i);
            }
            i = end;
        }
        Err(Error::Denied)
    }
    pub fn bytes(&self) -> usize {
        self.0.capacity()
    }
}
#[derive(Debug, Copy, Clone)]
pub struct Leaf {
    pub(crate) lease: Lease,
    pub(crate) epoch: u64,
    pub(crate) offset: usize,
    pub(crate) right: Right,
}

/// Objet logique d'une classe derrière le propriétaire du contrôleur.
#[derive(Debug, Copy, Clone)]
pub struct ClassLeaf {
    pub(crate) stamp: crate::lifecycle::Stamp,
    pub(crate) parent: Lease,
    pub(crate) epoch: u64,
    pub(crate) subject: u64,
    pub(crate) incarnation: u64,
}
