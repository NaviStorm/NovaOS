use super::*;

use serde::Deserializer;
pub const MAX_MANIFEST: usize = 16384;

pub(crate) fn hexbytes<'de, D: Deserializer<'de>, const N: usize>(
    d: D,
) -> core::result::Result<[u8; N], D::Error> {
    let s = String::deserialize(d)?;
    let mut out = [0; N];
    if s.len() != 2 * N
        || !s
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(serde::de::Error::custom("empreinte invalide"));
    }
    for (i, v) in out.iter_mut().enumerate() {
        *v = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).map_err(serde::de::Error::custom)?;
    }
    if out == [0; N] {
        return Err(serde::de::Error::custom("identifiant nul"));
    }
    Ok(out)
}
pub(crate) fn decode<T: crate::decode::Read>(b: &[u8]) -> Result<T> {
    crate::decode::decode(b, crate::MAX_DECODE_HEAP)
}
fn unique<I: Iterator + Clone>(seq: I) -> bool
where
    I::Item: PartialEq,
{
    seq.clone()
        .enumerate()
        .all(|(i, v)| !seq.clone().take(i).any(|old| old == v))
}
fn names(items: &[String]) -> bool {
    items.len() <= 32 && items.iter().all(|s| name(s)) && unique(items.iter())
}
pub(crate) fn deps(ds: &[Dependency]) -> bool {
    ds.len() <= 16
        && ds.iter().all(|d| name(&d.id) && d.min < d.before)
        && unique(ds.iter().map(|d| &d.id))
}
fn interface(s: &str) -> bool {
    s.split_once('/').is_some_and(|(n, v)| {
        name(n)
            && v.split_once('.').is_some_and(|(a, b)| {
                [a, b]
                    .iter()
                    .all(|s| s.parse::<u16>().is_ok() && s.bytes().all(|c| c.is_ascii_digit()))
            })
    })
}
impl Manifest {
    pub(crate) fn validate(&self) -> Result<()> {
        let c = &self.compat;
        let q = &self.quotas;
        if self.image.len() > 8
            || self.matches.len() > 32
            || self.dependency.len() > 16
            || self.required_caps.len() + self.optional_caps.len() > 32
            || q.ipc_roots_created > 4
            || q.ipc_boxes_created > 8
            || q.ipc_handles > 32
            || q.ipc_roots_created > q.ipc_handles
        {
            return Err(Error::Limit);
        }
        if self.schema != "novaos.driver/1"
            || !name(&self.id)
            || !name(&self.publisher)
            || !name(&self.signing_key_id)
            || !self
                .id
                .strip_prefix(&self.publisher)
                .is_some_and(|r| r.starts_with('.') && r.len() > 1)
            || !name(&self.class)
            || self.security_epoch == 0
            || self.elf_package_revision == 0
            || self.provides.is_empty()
            || self.provides.len() > 32
            || !self.provides.iter().all(|s| interface(s))
            || !unique(self.provides.iter())
            || self.matches.is_empty()
            || !self
                .matches
                .iter()
                .all(|b| matches!(b, Bus::Unknown) || b.valid())
            || c.abi_major == 0
            || c.os_min >= c.os_before
            || !names(&c.required_features)
            || !names(&self.required_caps)
            || !names(&self.optional_caps)
            || self
                .optional_caps
                .iter()
                .any(|v| self.required_caps.contains(v))
            || self.image.is_empty()
            || self.image.len() != c.architectures.len()
            || !unique(c.architectures.iter())
            || !unique(self.image.iter().map(|i| &i.isa))
            || !self.image.iter().all(|i| {
                matches!(i.isa.as_str(), "aarch64" | "x86_64")
                    && c.architectures.contains(&i.isa)
                    && name(&i.member)
                    && i.member.len() <= 63
            })
            || !deps(&self.dependency)
        {
            return Err(Error::Format);
        }
        let h = &self.hardware;
        if !names(&h.mmio)
            || !names(&h.irq)
            || h.dma.len() > 32
            || !unique(h.dma.iter().map(|r| r.split(':').next()))
            || !h.dma.iter().all(|s| {
                s.split_once(':')
                    .is_some_and(|(role, d)| name(role) && matches!(d, "r" | "w" | "rw"))
            })
            || h.irq.is_empty() != (h.max_irq_routes == 0)
            || h.dma.is_empty() != (h.dma_pages == 0)
            || (h.queues == 0) != (h.entries_per_queue == 0)
        {
            return Err(Error::Format);
        }
        Ok(())
    }
}
