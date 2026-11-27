use crate::{Dependency, Error, Kind, Resource, Result, Version};
use alloc::{string::String, vec::Vec};

/// Vue canonique d'une dépendance, copiée dans le plan en ordre d'identifiant.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Binding {
    pub id: String,
    pub kind: Kind,
    pub version: Version,
    pub sha256: [u8; 32],
    pub record_digest: [u8; 32],
}

/// Les ressources proviennent du banc de confiance, séparément des candidats.
pub fn resolve(needs: &[Dependency], resources: &[Resource]) -> Result<Vec<Binding>> {
    if needs.len() > 16 || resources.len() > 64 {
        return Err(Error::Limit);
    }
    let mut done = Vec::new();
    let mut active = Vec::new();
    let mut edges = 0usize;
    for need in needs {
        visit(need, resources, &mut done, &mut active, &mut edges)?;
    }
    done.sort_unstable_by(|a, b| a.id.cmp(&b.id));
    Ok(done)
}

fn visit<'a>(
    need: &Dependency,
    resources: &'a [Resource],
    done: &mut Vec<Binding>,
    active: &mut Vec<&'a str>,
    edges: &mut usize,
) -> Result<()> {
    *edges += 1;
    if *edges > 64 {
        return Err(Error::Limit);
    }
    let Some(node) = resources.iter().find(|n| {
        n.id == need.id
            && n.kind == need.kind
            && need.min <= n.version
            && n.version < need.before
            && need.sha256 == n.digest
    }) else {
        return if need.required {
            Err(Error::Dependency)
        } else {
            Ok(())
        };
    };
    if active.contains(&node.id.as_str()) {
        return Err(Error::Cycle);
    }
    if let Some(old) = done.iter().find(|b| b.id == node.id) {
        if old.sha256 != node.digest || old.version != node.version || old.kind != node.kind {
            return Err(Error::Conflict);
        }
        return Ok(());
    }
    if node.dependencies.len() > 16 {
        return Err(Error::Limit);
    }
    active.try_reserve_exact(1).map_err(|_| Error::NoMemory)?;
    active.push(&node.id);
    for child in &node.dependencies {
        visit(child, resources, done, active, edges)?;
    }
    active.pop();
    done.try_reserve_exact(1).map_err(|_| Error::NoMemory)?;
    done.push(Binding {
        id: crate::bounded::copy_str(&node.id)?,
        version: node.version,
        kind: node.kind,
        sha256: node.digest,
        record_digest: node.record_digest,
    });
    Ok(())
}

#[cfg(test)]
mod tests;
