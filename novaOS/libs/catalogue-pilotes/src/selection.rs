use crate::{Binding, Bus, Catalog, Error, Limits, Result, Version, resolve};
use alloc::{string::String, vec::Vec};

/// Instantané explicitement fourni par l'autorité du banc.
pub struct Target<'a> {
    pub revision: u64,
    /// Profil système dont proviennent les plafonds matériels injectés.
    pub profile: Option<[u8; 32]>,
    pub attachment: &'a str,
    pub bus: Bus,
    pub class: &'a str,
    pub isa: &'a str,
    pub os: Version,
    pub abi: u32,
    pub features: &'a [&'a str],
    pub caps: &'a [&'a str],
    pub dma: &'a [&'a str],
    pub mmio: &'a [&'a str],
    pub irq: &'a [&'a str],
    pub limits: Limits,
    pub allow_class_match: bool,
}
/// Le résultat possède ses données. Toute nouvelle politique exige une nouvelle sélection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    driver: String,
    version: Version,
    manifest: [u8; 32],
    executable: [u8; 32],
    object: [u8; 32],
    authority_revision: u64,
    inventory_revision: u64,
    attachment: String,
    isa: String,
    hardware: crate::Hardware,
    quotas: crate::Quotas,
    caps: Vec<String>,
    resources: Vec<Binding>,
}
impl Plan {
    pub fn driver(&self) -> &str {
        &self.driver
    }
    pub fn version(&self) -> Version {
        self.version
    }
    pub fn hardware(&self) -> &crate::Hardware {
        &self.hardware
    }
    pub fn quotas(&self) -> &crate::Quotas {
        &self.quotas
    }
    pub fn capabilities(&self) -> &[String] {
        &self.caps
    }
    pub fn dependencies(&self) -> &[Binding] {
        &self.resources
    }
    pub fn manifest_digest(&self) -> [u8; 32] {
        self.manifest
    }
    pub fn image_digest(&self) -> [u8; 32] {
        self.executable
    }
    pub fn package_digest(&self) -> [u8; 32] {
        self.object
    }
    pub fn revisions(&self) -> (u64, u64) {
        (self.authority_revision, self.inventory_revision)
    }
    pub fn attachment(&self) -> &str {
        &self.attachment
    }
    pub fn architecture(&self) -> &str {
        &self.isa
    }
}

impl Catalog<'_> {
    /// Une épingle absente ou inéligible bloque entièrement la sélection.
    pub fn select(&self, t: &Target<'_>, pin: Option<[u8; 32]>) -> Result<Plan> {
        if t.revision == 0 || !crate::name(t.attachment) || !t.bus.valid() {
            return Err(Error::Format);
        }
        let mut best = None;
        let mut conflict = false;
        let mut failure = None;
        // L'index ordonné rend aussi le diagnostic déterministe en cas de refus multiples.
        for entry in &self.entries {
            if pin.is_some_and(|v| v != entry.digest) {
                continue;
            }
            let m = &entry.manifest;
            if m.class != t.class
                || m.compat.abi_major != t.abi
                || t.os < m.compat.os_min
                || t.os >= m.compat.os_before
            {
                continue;
            }
            let Some(image) = m.image.iter().find(|i| i.isa == t.isa) else {
                continue;
            };
            let Some(score) = m.matches.iter().filter_map(|v| v.specificity(&t.bus)).max() else {
                continue;
            };
            if score < 8 && !t.allow_class_match {
                continue;
            }
            if !m
                .compat
                .required_features
                .iter()
                .all(|v| t.features.contains(&v.as_str()))
            {
                continue;
            }
            if !m.required_caps.iter().all(|v| t.caps.contains(&v.as_str()))
                || !m.hardware.dma.iter().all(|v| t.dma.contains(&v.as_str()))
                || !m.hardware.mmio.iter().all(|v| t.mmio.contains(&v.as_str()))
                || !m.hardware.irq.iter().all(|v| t.irq.contains(&v.as_str()))
            {
                failure.get_or_insert(Error::Denied);
                continue;
            }
            if !t.limits.fits(&m.quotas, &m.hardware) {
                failure.get_or_insert(Error::Limit);
                continue;
            }
            let bindings = match resolve(&m.dependency, &self.policy.resources) {
                Ok(result) => result,
                Err(e) => {
                    failure.get_or_insert(e);
                    continue;
                }
            };
            let h = &m.hardware;
            if (!h.dma.is_empty() || !h.irq.is_empty() || !h.mmio.is_empty() || h.queues > 0)
                && !m.dependency.iter().any(|d| {
                    d.required
                        && d.kind == crate::Kind::SystemProfile
                        && Some(d.sha256) == t.profile
                })
            {
                failure.get_or_insert(Error::Denied);
                continue;
            }
            let available = |d: &crate::Dependency| {
                bindings.iter().any(|b| {
                    b.id == d.id
                        && b.kind == d.kind
                        && b.sha256 == d.sha256
                        && d.min <= b.version
                        && b.version < d.before
                })
            };
            let options_ready = m.dependency.iter().all(available)
                && self
                    .policy
                    .resources
                    .iter()
                    .filter(|r| bindings.iter().any(|b| b.record_digest == r.record_digest))
                    .all(|r| r.dependencies.iter().all(available));
            let rank = (entry.priority, score);
            // Les identités sont contiguës dans l'index. Une égalité entre identités
            // ne disparaît qu'à l'arrivée d'un rang strictement supérieur.
            if let Some((old_rank, ref old)) = best {
                let old: &Plan = old;
                if rank < old_rank {
                    continue;
                }
                if rank == old_rank {
                    if old.driver != m.id {
                        conflict = true;
                        continue;
                    }
                    if old.version >= m.version {
                        continue;
                    }
                } else {
                    conflict = false;
                }
            }
            let mut caps = m.required_caps.clone();
            caps.extend(
                m.optional_caps
                    .iter()
                    .filter(|v| options_ready && t.caps.contains(&v.as_str()))
                    .cloned(),
            );
            caps.sort();
            best = Some((
                rank,
                Plan {
                    driver: m.id.clone(),
                    version: m.version,
                    manifest: entry.digest,
                    executable: image.elf_sha256,
                    object: image.package_sha256,
                    authority_revision: self.policy.revision,
                    inventory_revision: t.revision,
                    attachment: t.attachment.into(),
                    isa: t.isa.into(),
                    hardware: m.hardware.clone(),
                    quotas: m.quotas.clone(),
                    caps,
                    resources: bindings,
                },
            ));
        }
        if conflict {
            return Err(Error::Conflict);
        }
        best.map(|(_, p)| p)
            .ok_or(failure.unwrap_or(Error::NoMatch))
    }
}

#[cfg(test)]
mod tests;
