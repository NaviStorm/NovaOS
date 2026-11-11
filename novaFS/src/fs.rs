//! Catalogue, hiérarchie et primitives locales du moteur sur image expérimentale.
use alloc::{borrow::ToOwned, string::String, vec::Vec};
mod check;
mod data;
mod record;
use crate::{
    Device, Engine, Error, Result, Tx, View,
    codec::Ref,
    tree::{self, Family},
};
pub(crate) use check::{check_shared_data, data_pages, validate};
pub(crate) use data::ReadCursor;
use record::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    File,
    Directory,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stat {
    pub id: u64,
    pub kind: Kind,
    pub size: u64,
    pub revision: u64,
}

fn generation(v: &impl View) -> Result<u64> {
    v.manifest()
        .previous
        .checked_add(1)
        .ok_or(Error::Corrupt("génération du manifeste"))
}
fn root(v: &impl View, f: Family) -> Ref {
    v.manifest().roots[f.index()]
}
fn get(v: &impl View, f: Family, key: &[u8]) -> Result<Option<Vec<u8>>> {
    tree::get(v, root(v, f), f, key)
}
fn object(v: &impl View, id: u64) -> Result<Object> {
    let data = get(v, Family::Objects, &id.to_le_bytes())?
        .ok_or(Error::Corrupt("objet référencé absent"))?;
    Object::decode(id, &data, generation(v)?, v.manifest().atime.is_some())
}
fn live(v: &impl View, id: u64) -> Result<Object> {
    let obj = object(v, id)?;
    if obj.dead {
        return Err(Error::Corrupt("objet retiré accessible"));
    }
    Ok(obj)
}
fn save_object<D: Device>(tx: &mut Tx<D>, obj: &Object) -> Result<()> {
    tx.put(
        Family::Objects,
        obj.stat.id.to_le_bytes().to_vec(),
        obj.encode(tx.manifest().atime.is_some()),
    )
}
fn touch<D: Device>(tx: &mut Tx<D>, id: u64) -> Result<()> {
    let mut obj = live(tx, id)?;
    obj.stat.revision = generation(tx)?;
    obj.times.mtime = tx.now;
    obj.times.ctime = tx.now;
    save_object(tx, &obj)
}
pub(crate) fn initialize<D: Device>(tx: &mut Tx<D>) -> Result<()> {
    tx.manifest_mut().root_oid = ROOT;
    tx.manifest_mut().next_oid = 2;
    let mut root = Object::new(ROOT, Kind::Directory, ROOT, "", generation(tx)?);
    root.times = crate::time::Times::created(tx.now);
    save_object(tx, &root)
}

fn components(path: &str) -> Result<Vec<&str>> {
    if path.is_empty() || path.len() > 1024 || path.as_bytes().contains(&0) {
        return Err(Error::Invalid("chemin"));
    }
    if path == "/" {
        return Ok(Vec::new());
    }
    let mut steps = Vec::new();
    for name in path.strip_prefix('/').unwrap_or(path).split('/') {
        if steps.len() == MAX_DEPTH || name.is_empty() || name.len() > 255 {
            return Err(Error::Invalid("composants du chemin"));
        }
        steps.push(name);
    }
    Ok(steps)
}

fn resolve(v: &impl View, steps: &[&str]) -> Result<Object> {
    resolve_from(v, ROOT, steps)
}

// Un identifiant fourni par l'appelant peut avoir été supprimé. Une cible
// absente dans l'index des noms, en revanche, est une corruption (live).
fn by_id(v: &impl View, id: u64) -> Result<Object> {
    if id == ROOT {
        return live(v, id);
    }
    let bytes = get(v, Family::Objects, &id.to_le_bytes())?.ok_or(Error::NotFound)?;
    let obj = Object::decode(id, &bytes, generation(v)?, v.manifest().atime.is_some())?;
    if obj.dead {
        Err(Error::NotFound)
    } else {
        Ok(obj)
    }
}

fn relative(path: &str) -> Result<&str> {
    if path.starts_with('/') || path.split('/').any(|s| s == "..") {
        return Err(Error::Invalid("sortie du répertoire"));
    }
    Ok(path)
}

fn resolve_from(v: &impl View, base: u64, steps: &[&str]) -> Result<Object> {
    let mut current = by_id(v, base)?;
    for &step in steps {
        if current.stat.kind != Kind::Directory {
            return Err(Error::NotDirectory);
        }
        current = match step {
            "." => current,
            ".." => live(v, current.parent)?,
            _ => {
                let child = get(v, Family::Names, &name_key(current.stat.id, step))?
                    .ok_or(Error::NotFound)?;
                let obj = live(v, number(&child)?)?;
                if obj.parent != current.stat.id || obj.name != step {
                    return Err(Error::Corrupt("lien de nom incohérent"));
                }
                obj
            }
        }
    }
    Ok(current)
}

fn directory_and_leaf<'p>(v: &impl View, base: u64, path: &'p str) -> Result<(Object, &'p str)> {
    let steps = components(path)?;
    let (name, parent_steps) = steps
        .split_last()
        .ok_or(Error::Invalid("racine immuable"))?;
    if !valid_name(name) {
        return Err(Error::Invalid("nom réservé"));
    }
    let parent = resolve_from(v, base, parent_steps)?;
    if parent.stat.kind != Kind::Directory {
        return Err(Error::NotDirectory);
    }
    Ok((parent, *name))
}

fn depth(v: &impl View, start: &Object, forbidden: Option<u64>) -> Result<usize> {
    let mut count = 0;
    let mut node = start.clone();
    loop {
        if Some(node.stat.id) == forbidden {
            return Err(Error::Invalid("cycle de répertoire"));
        }
        if node.stat.id == ROOT {
            return Ok(count);
        }
        if count == MAX_DEPTH {
            return Err(Error::Corrupt("cycle ou profondeur du catalogue"));
        }
        node = live(v, node.parent)?;
        if node.stat.kind != Kind::Directory {
            return Err(Error::Corrupt("parent non répertoire"));
        }
        count += 1;
    }
}

fn directory_entry(
    v: &impl View,
    parent: u64,
    after: Option<&str>,
) -> Result<Option<(String, u64)>> {
    let f = Family::Names;
    let entry = match after {
        None => tree::lower_bound(v, root(v, f), f, &name_key(parent, ""))?,
        Some(name) => tree::next(v, root(v, f), f, &name_key(parent, name))?,
    };
    match entry {
        None => Ok(None),
        Some((key, value)) => {
            let (found, name) = parse_name_key(&key)?;
            if found != parent {
                return Ok(None);
            }
            Ok(Some((name.to_owned(), number(&value)?)))
        }
    }
}

/// Un déplacement vers une profondeur plus grande exige de certifier toute la
/// hauteur restante. La recherche en profondeur garde seulement des curseurs
/// de noms, avec une borne de travail pour les sous-arbres pathologiques.
fn fits_at(v: &impl View, id: u64, level: usize, fuel: &mut usize) -> Result<()> {
    if level > MAX_DEPTH {
        return Err(Error::Limit("profondeur après renommage"));
    }
    let mut previous: Option<String> = None;
    while let Some((name, child)) = directory_entry(v, id, previous.as_deref())? {
        *fuel = fuel
            .checked_sub(1)
            .ok_or(Error::Limit("inspection de renommage"))?;
        if level == MAX_DEPTH {
            return Err(Error::Limit("profondeur après renommage"));
        }
        if live(v, child)?.stat.kind == Kind::Directory {
            fits_at(v, child, level + 1, fuel)?;
        }
        previous = Some(name);
    }
    Ok(())
}

impl<D: Device> Engine<D> {
    fn checked_access<T>(&self, work: impl FnOnce() -> Result<T>) -> Result<T> {
        self.observe(Ok(()))?;
        self.observe(work())
    }

    pub fn stat(&self, path: &str) -> Result<Stat> {
        self.checked_access(|| Ok(resolve(self, &components(path)?)?.stat))
    }

    /// L'autorité sur base appartient au service appelant. Les liens descendants
    /// sont recoupés avec l'enregistrement de l'objet à chaque étape.
    pub fn lookup_at(&self, base: u64, path: &str) -> Result<Stat> {
        self.checked_access(|| Ok(resolve_from(self, base, &components(relative(path)?)?)?.stat))
    }

    pub fn stat_id(&self, id: u64) -> Result<Stat> {
        self.checked_access(|| Ok(by_id(self, id)?.stat))
    }

    fn legacy_read(&self) -> Result<()> {
        if !self.read_only()
            && self
                .atime_policy()
                .is_some_and(|p| p != crate::time::Atime::No)
        {
            return Err(Error::Invalid(
                "accès P9 inscriptible : utiliser read_access ou list_access",
            ));
        }
        Ok(())
    }
    pub fn times(&self, path: &str) -> Result<crate::time::Times> {
        self.checked_access(|| Ok(resolve(self, &components(path)?)?.times))
    }

    /// L'appelant valide sa capacité sur l'objet avant d'utiliser son identifiant.
    pub fn times_id(&self, id: u64) -> Result<crate::time::Times> {
        self.checked_access(|| Ok(by_id(self, id)?.times))
    }

    pub fn set_times(
        &mut self,
        path: &str,
        atime: Option<crate::time::Timestamp>,
        mtime: Option<crate::time::Timestamp>,
    ) -> Result<()> {
        self.update_times(|tx| resolve(tx, &components(path)?), atime, mtime)
    }

    pub fn set_times_id(
        &mut self,
        id: u64,
        atime: Option<crate::time::Timestamp>,
        mtime: Option<crate::time::Timestamp>,
    ) -> Result<()> {
        self.update_times(|tx| by_id(tx, id), atime, mtime)
    }

    fn update_times(
        &mut self,
        object: impl FnOnce(&Tx<'_, D>) -> Result<Object>,
        atime: Option<crate::time::Timestamp>,
        mtime: Option<crate::time::Timestamp>,
    ) -> Result<()> {
        if self.atime_policy().is_none() {
            return Err(Error::Invalid("volume P9 requis"));
        }
        self.transact(false, |tx| {
            let mut obj = object(tx)?;
            let before = obj.times;
            if let Some(value) = atime {
                obj.times.atime = value;
            }
            if let Some(value) = mtime {
                obj.times.mtime = value;
            }
            if obj.times != before {
                obj.times.ctime = tx.now;
                obj.stat.revision = generation(tx)?;
                save_object(tx, &obj)?;
            }
            Ok(())
        })
    }

    fn record_access(&mut self, id: u64) -> Result<()> {
        let Some(policy) = self.atime_policy() else {
            return Ok(());
        };
        if self.read_only() || policy == crate::time::Atime::No {
            return Ok(());
        }
        self.transact(false, |tx| {
            let mut obj = by_id(tx, id)?;
            if policy.update(obj.times, tx.now) {
                obj.times.atime = tx.now;
                obj.stat.revision = generation(tx)?;
                save_object(tx, &obj)?;
            }
            Ok(())
        })
    }

    /// Lecture applicative. Les vérificateurs utilisent inspect_id.
    pub fn read_access(&mut self, path: &str, offset: u64, limit: usize) -> Result<Vec<u8>> {
        let id = self.stat(path)?.id;
        self.read_access_id(id, offset, limit)
    }

    pub fn read_access_id(&mut self, id: u64, offset: u64, limit: usize) -> Result<Vec<u8>> {
        let data = self.inspect_id(id, offset, limit)?;
        if !data.is_empty() {
            self.record_access(id)?;
        }
        Ok(data)
    }

    pub fn list_access(
        &mut self,
        path: &str,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<String>> {
        let id = self.stat(path)?.id;
        self.list_access_id(id, after, limit)
    }

    pub fn list_access_id(
        &mut self,
        id: u64,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<String>> {
        let entries = self.inspect_dir(id, after, limit)?;
        if limit != 0 {
            self.record_access(id)?;
        }
        Ok(entries)
    }

    pub const MAX_DIRECTORY_DEPTH: usize = MAX_DEPTH;

    pub fn root_id(&self) -> u64 {
        ROOT
    }

    /// Lien inverse validé au montage, pour l'affichage du répertoire courant.
    pub fn parent_name(&self, id: u64) -> Result<(u64, String)> {
        self.checked_access(|| {
            let obj = by_id(self, id)?;
            Ok((obj.parent, obj.name))
        })
    }

    /// Page de noms triés. Le nom de continuation est le dernier nom rendu.
    pub fn list(&self, path: &str, after: Option<&str>, limit: usize) -> Result<Vec<String>> {
        self.legacy_read()?;
        self.checked_access(|| {
            self.directory_names(resolve(self, &components(path)?)?.stat, after, limit)
        })
    }

    pub fn list_id(&self, id: u64, after: Option<&str>, limit: usize) -> Result<Vec<String>> {
        self.legacy_read()?;
        self.inspect_dir(id, after, limit)
    }
    pub fn inspect_dir(&self, id: u64, after: Option<&str>, limit: usize) -> Result<Vec<String>> {
        self.checked_access(|| self.directory_names(by_id(self, id)?.stat, after, limit))
    }

    fn directory_names(&self, dir: Stat, after: Option<&str>, limit: usize) -> Result<Vec<String>> {
        if limit > 64 || after.is_some_and(|s| !valid_name(s)) {
            return Err(Error::Invalid("page de répertoire"));
        }
        if dir.kind != Kind::Directory {
            return Err(Error::NotDirectory);
        }
        let mut names: Vec<String> = Vec::with_capacity(limit);
        for _ in 0..limit {
            let cursor = names.last().map(String::as_str).or(after);
            let Some((name, _)) = directory_entry(self, dir.id, cursor)? else {
                break;
            };
            names.push(name);
        }
        Ok(names)
    }

    pub fn create(&mut self, path: &str) -> Result<()> {
        self.create_object(ROOT, path, Kind::File).map(|_| ())
    }
    pub fn mkdir(&mut self, path: &str) -> Result<()> {
        self.create_object(ROOT, path, Kind::Directory).map(|_| ())
    }

    pub fn create_at(&mut self, base: u64, path: &str, kind: Kind) -> Result<Stat> {
        self.create_object(base, relative(path)?, kind)
    }

    fn create_object(&mut self, base: u64, path: &str, kind: Kind) -> Result<Stat> {
        self.transact(false, |tx| {
            let (dir, name) = directory_and_leaf(tx, base, path)?;
            let key = name_key(dir.stat.id, name);
            if get(tx, Family::Names, &key)?.is_some() {
                return Err(Error::Exists);
            }
            if depth(tx, &dir, None)? == MAX_DEPTH {
                return Err(Error::Limit("profondeur maximale"));
            }
            let id = tx.manifest().next_oid;
            tx.manifest_mut().next_oid =
                id.checked_add(1).ok_or(Error::Limit("numéros d’objet"))?;
            let mut obj = Object::new(id, kind, dir.stat.id, name, generation(tx)?);
            obj.times = crate::time::Times::created(tx.now);
            save_object(tx, &obj)?;
            tx.put(Family::Names, key, id.to_le_bytes().to_vec())?;
            touch(tx, dir.stat.id)?;
            Ok(obj.stat)
        })
    }

    pub fn rename(&mut self, from: &str, to: &str) -> Result<()> {
        self.move_entry(ROOT, from, ROOT, to)
    }

    pub fn rename_at(&mut self, src: u64, from: &str, dst: u64, to: &str) -> Result<()> {
        self.move_entry(src, relative(from)?, dst, relative(to)?)
    }

    fn move_entry(&mut self, src: u64, from: &str, dst: u64, to: &str) -> Result<()> {
        self.transact(false, |tx| {
            let (before, old_name) = directory_and_leaf(tx, src, from)?;
            let (after, new_name) = directory_and_leaf(tx, dst, to)?;
            let old_key = name_key(before.stat.id, old_name);
            let new_key = name_key(after.stat.id, new_name);
            let id = number(&get(tx, Family::Names, &old_key)?.ok_or(Error::NotFound)?)?;
            if old_key == new_key {
                return Ok(());
            }
            if get(tx, Family::Names, &new_key)?.is_some() {
                return Err(Error::Exists);
            }
            let mut obj = live(tx, id)?;
            let target_depth = depth(tx, &after, Some(id))? + 1;
            if target_depth > MAX_DEPTH {
                return Err(Error::Limit("profondeur maximale"));
            }
            if obj.stat.kind == Kind::Directory && target_depth > depth(tx, &obj, None)? {
                let mut fuel = 16384;
                fits_at(tx, id, target_depth, &mut fuel)?;
            }
            obj.parent = after.stat.id;
            obj.name = new_name.to_owned();
            obj.stat.revision = generation(tx)?;
            obj.times.ctime = tx.now;
            tx.remove(Family::Names, &old_key)?;
            tx.put(Family::Names, new_key, id.to_le_bytes().to_vec())?;
            save_object(tx, &obj)?;
            touch(tx, before.stat.id)?;
            if before.stat.id != after.stat.id {
                touch(tx, after.stat.id)?;
            }
            Ok(())
        })
    }

    pub fn unlink(&mut self, path: &str) -> Result<()> {
        self.remove_entry(ROOT, path, None)
    }

    pub fn unlink_at(&mut self, base: u64, path: &str, kind: Kind) -> Result<()> {
        self.remove_entry(base, relative(path)?, Some(kind))
    }

    fn remove_entry(&mut self, base: u64, path: &str, kind: Option<Kind>) -> Result<()> {
        self.transact(true, |tx| {
            let (dir, name) = directory_and_leaf(tx, base, path)?;
            let key = name_key(dir.stat.id, name);
            let id = number(&get(tx, Family::Names, &key)?.ok_or(Error::NotFound)?)?;
            let mut obj = live(tx, id)?;
            if kind.is_some_and(|k| k != obj.stat.kind) {
                return Err(if obj.stat.kind == Kind::Directory {
                    Error::IsDirectory
                } else {
                    Error::NotDirectory
                });
            }
            if obj.stat.kind == Kind::Directory && directory_entry(tx, id, None)?.is_some() {
                return Err(Error::NotEmpty);
            }
            tx.resize_logical(obj.stat.size, 0)?;
            tx.remove(Family::Names, &key)?;
            if let Some((first, _)) = data::first_extent(tx, id, 0)? {
                obj.dead = true;
                obj.parent = 0;
                obj.name.clear();
                obj.stat.revision = generation(tx)?;
                obj.times.ctime = tx.now;
                save_object(tx, &obj)?;
                tx.put(Family::Garbage, id.to_le_bytes().to_vec(), gc_value(first))?;
            } else {
                tx.remove(Family::Objects, &id.to_le_bytes())?;
            }
            touch(tx, dir.stat.id)
        })
    }

    pub fn collection_pending(&self) -> Result<bool> {
        let f = Family::Garbage;
        self.checked_access(|| Ok(tree::first(self, root(self, f), f)?.is_some()))
    }

    /// Libère un extent de l'objet en tête et le retire avec son dernier lot.
    pub fn collect(&mut self) -> Result<bool> {
        let f = Family::Garbage;
        self.transact(true, |tx| {
            let Some((k, value)) = tree::first(tx, root(tx, f), f)? else {
                return Ok(false);
            };
            let id = number(&k)?;
            let cursor = gc_cursor(&value)?;
            let (logical, run) = data::first_extent(tx, id, cursor)?
                .ok_or(Error::Corrupt("lot de collecte absent"))?;
            if logical != cursor {
                return Err(Error::Corrupt("curseur de collecte incohérent"));
            }
            for index in 0..run.hashes.len() {
                tx.retire_block(run.first + index as u64)?;
            }
            tx.remove(Family::Extents, &extent_key(id, logical))?;
            if let Some((next, _)) = data::first_extent(tx, id, cursor)? {
                tx.put(f, k, gc_value(next))?;
            } else {
                tx.remove(f, &k)?;
                tx.remove(Family::Objects, &id.to_le_bytes())?;
            }
            Ok(true)
        })
    }
}

#[cfg(test)]
mod tests;
