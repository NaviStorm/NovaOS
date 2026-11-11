//! Contrôle logique des quatre index de fichiers. Les bitmaps de propriété et
//! l'arbre d'espace sont vérifiés par le moteur de montage dans la même vue.
use super::*;

fn walk(v: &impl View, f: Family, entry: impl FnMut(&[u8], &[u8]) -> Result<()>) -> Result<()> {
    tree::walk(v, root(v, f), f, |_| Ok(()), entry)
}

pub(crate) fn validate<V: View, W: View>(
    v: &V,
    older: Option<&W>,
    full: bool,
    on_data: &mut impl FnMut(Ref) -> Result<()>,
) -> Result<u128> {
    let m = v.manifest();
    let view_gen = generation(v)?;
    if m.root_oid != ROOT
        || m.next_oid <= ROOT
        || older.is_some_and(|w| m.next_oid < w.manifest().next_oid)
    {
        return Err(Error::Corrupt("racine ou recul du compteur d’objet"));
    }
    let _ = live(v, ROOT)?;
    let mut logical_bytes = 0u128;
    walk(v, Family::Objects, |key, record| {
        let id = number(key)?;
        if id >= m.next_oid {
            return Err(Error::Corrupt("numéro d’objet hors compteur"));
        }
        let obj = Object::decode(id, record, view_gen, m.atime.is_some())?;
        if !obj.dead && obj.stat.kind == Kind::File {
            logical_bytes = logical_bytes
                .checked_add(u128::from(obj.stat.size))
                .ok_or(Error::Corrupt("somme des tailles logiques"))?;
        }
        if let Some(old) = older
            && id < old.manifest().next_oid
        {
            let saved = get(old, Family::Objects, key)?
                .ok_or(Error::Corrupt("numéro d’objet réutilisé"))?;
            let previous =
                Object::decode(id, &saved, generation(old)?, old.manifest().atime.is_some())?;
            if (previous.dead && !obj.dead)
                || previous.stat.kind != obj.stat.kind
                || previous.times.btime != obj.times.btime
                || obj.stat.revision < previous.stat.revision
                || (record != saved && obj.stat.revision != view_gen)
            {
                return Err(Error::Corrupt("changement d’identité ou révision d’objet"));
            }
        }
        if older.is_some_and(|old| id >= old.manifest().next_oid) && obj.stat.revision != view_gen {
            return Err(Error::Corrupt(
                "naissance d’objet non attribuable au commit",
            ));
        }
        if obj.dead {
            let gc =
                get(v, Family::Garbage, key)?.ok_or(Error::Corrupt("objet retiré sans curseur"))?;
            gc_cursor(&gc)?;
        } else {
            if id != ROOT {
                let name_ref = get(v, Family::Names, &name_key(obj.parent, &obj.name))?;
                if name_ref.as_deref() != Some(key) {
                    return Err(Error::Corrupt("entrée inverse manquante"));
                }
            }
            depth(v, &obj, None)?;
        }
        Ok(())
    })?;
    walk(v, Family::Names, |key, value| {
        let (parent, name) = parse_name_key(key)?;
        let obj = live(v, number(value)?)?;
        if obj.stat.id == ROOT
            || obj.parent != parent
            || obj.name != name
            || live(v, parent)?.stat.kind != Kind::Directory
        {
            return Err(Error::Corrupt("index des noms incohérent"));
        }
        Ok(())
    })?;
    // Le parcours ordonné permet de détecter un recouvrement en un seul passage.
    let mut last_id = 0;
    let mut after = 0;
    walk(v, Family::Extents, |key, value| {
        let (id, first) = parse_extent_key(key)?;
        if last_id == id && first < after {
            return Err(Error::Corrupt("recouvrement logique d’extents"));
        }
        let obj = object(v, id)?;
        let run = Run::decode(value, m.blocks, view_gen)?;
        if run.birth > obj.stat.revision {
            return Err(Error::Corrupt("extent postérieur à la révision du fichier"));
        }
        after = run.end(first)?;
        last_id = id;
        if obj.stat.kind != Kind::File || after > obj.stat.size.div_ceil(BLOCK_BYTES) {
            return Err(Error::Corrupt("extent au-delà du fichier"));
        }
        for i in 0..run.hashes.len() {
            let reference = run.reference(i);
            on_data(reference)?;
            if full {
                data::checked_page(v, reference, first + i as u64, obj.stat.size)?;
            }
        }
        Ok(())
    })?;
    walk(v, Family::Garbage, |key, value| {
        let id = number(key)?;
        let cursor = gc_cursor(value)?;
        let first = data::first_extent(v, id, 0)?;
        if !object(v, id)?.dead || first.as_ref().map(|e| e.0) != Some(cursor) {
            return Err(Error::Corrupt("curseur de collecte sans lot correspondant"));
        }
        Ok(())
    })?;
    Ok(logical_bytes)
}

/// Visite les LBA de payloads d'un arbre d'extents déjà admis par le montage.
pub(crate) fn data_pages(v: &impl View, mut visit: impl FnMut(u64) -> Result<()>) -> Result<()> {
    walk(v, Family::Extents, |_, value| {
        let run = Run::decode(value, v.manifest().blocks, generation(v)?)?;
        for index in 0..run.hashes.len() {
            visit(run.reference(index).block)?;
        }
        Ok(())
    })
}

pub(crate) fn check_shared_data(
    v: &impl View,
    other: &impl View,
    shared: &impl Fn(u64) -> bool,
) -> Result<()> {
    walk(v, Family::Extents, |key, value| {
        let (id, first) = parse_extent_key(key)?;
        let run = Run::decode(value, v.manifest().blocks, generation(v)?)?;
        for i in 0..run.hashes.len() {
            let r = run.reference(i);
            if !shared(r.block) {
                continue;
            }
            let logical = first
                .checked_add(i as u64)
                .ok_or(Error::Corrupt("offset d’extent partagé"))?;
            let Some((start, other_run)) = data::covering(other, id, logical)? else {
                return Err(Error::Corrupt("autre propriétaire du bloc partagé"));
            };
            if other_run.reference((logical - start) as usize) != r {
                return Err(Error::Corrupt("bloc partagé réattribué ou modifié"));
            }
        }
        Ok(())
    })
}
