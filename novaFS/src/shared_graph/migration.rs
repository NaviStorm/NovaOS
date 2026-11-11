//! Réencodage des deux catalogues vivants. Une unité immuable est visitée une
//! fois, y compris quand plusieurs entrées d'une feuille désignent le même pack.
use super::*;
use zeroize::Zeroizing;

#[derive(Default)]
pub(crate) struct Remap(BTreeMap<u64, (Ids, PageRef, PageRef, Role)>);

impl Remap {
    pub(crate) fn addresses(&self) -> impl Iterator<Item = (u64, Ids)> + '_ {
        self.0.iter().map(|(&b, (ids, _, _, _))| (b, *ids))
    }
    pub(crate) fn identity(&self) -> (u64, Hash) {
        let mut h = Sha256::new();
        h.update(b"NovaFS/P9/correspondance des unites/v1");
        for (addr, (ids, old, new, _)) in &self.0 {
            h.update(ids.fs);
            h.update(ids.lv);
            h.update(addr.to_le_bytes());
            h.update(old.hash);
            h.update(new.hash);
        }
        (self.0.len() as u64, h.finalize().into())
    }

    pub(crate) fn view(
        &mut self,
        dev: &impl Device,
        e: &Entry,
        p6: bool,
        mut emit: impl FnMut(u64, Role, &Block) -> Result<Hash>,
    ) -> Result<Entry> {
        let mut out = e.clone();
        out.root = self.rewrite(dev, e.ids, e.root, Role::Root, p6, &mut emit)?;
        Ok(out)
    }

    fn rewrite(
        &mut self,
        dev: &impl Device,
        ids: Ids,
        old: PageRef,
        role: Role,
        p6: bool,
        emit: &mut impl FnMut(u64, Role, &Block) -> Result<Hash>,
    ) -> Result<PageRef> {
        if old == PageRef::EMPTY {
            return Ok(old);
        }
        if let Some((lineage, original, new, expected)) = self.0.get(&old.block) {
            if *lineage != ids || *original != old || *expected != role {
                return Err(Error::Corrupt("référence de migration ambiguë"));
            }
            return Ok(*new);
        }
        let mut plain = Zeroizing::new(checked_page(dev, old, role)?);
        if matches!(role, Role::Root | Role::Leaf(_)) {
            for (i, child) in pointers(dev, old, role, p6)?.into_iter().enumerate() {
                if child == PageRef::EMPTY {
                    continue;
                }
                let how = match role {
                    Role::Root => Role::Leaf(i as u16),
                    Role::Leaf(leaf) if child.packed => Role::Packed(leaf * WIDTH as u16),
                    Role::Leaf(leaf) => Role::Data(leaf * WIDTH as u16 + i as u16),
                    _ => unreachable!(),
                };
                let new = self.rewrite(dev, ids, child, how, p6, emit)?;
                let start = 32 + i * 40;
                new.write(&mut plain[start..start + 40]);
            }
            // Les rôles de P5 et P6 ont des en-têtes distincts.
            if !p6 {
                let magic = if role == Role::Root { ROOT } else { LEAF };
                plain[..16].copy_from_slice(&profile(magic, true)[..16]);
            }
        }
        let new = PageRef {
            hash: emit(old.block, role, &plain)?,
            ..old
        };
        self.0.insert(old.block, (ids, old, new, role));
        Ok(new)
    }
}

/// La destination connaît l'origine de sa première génération. L'ancien
/// numéro de génération et les deux racines source sont conservés par le reçu.
pub(crate) fn install(
    prefix: &mut [Block],
    cat: &Catalog,
    blocks: u64,
    parent: Hash,
) -> Result<Hash> {
    let first = cat.generation == 1;
    if prefix.len() != FIXED as usize || !cat.p6 || cat.generation > 2 {
        return Err(Error::Invalid("catalogues de migration"));
    }
    if first {
        prefix.fill([0; 4096]);
        prefix[0] = header(cat.pool, blocks, 0, true);
        prefix[1] = header(cat.pool, blocks, 1, true);
    }
    let (record, hash) = encode_catalog(cat, parent, blocks, first)?;
    prefix[(3 + cat.generation % 4) as usize] = record;
    prefix[2] = marker(cat.pool, cat.generation % 4, true);
    Ok(hash)
}
