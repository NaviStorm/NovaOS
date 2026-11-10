use super::*;
use crate::Block;

pub(super) fn first_extent(v: &impl View, id: u64, at: u64) -> Result<Option<(u64, Run)>> {
    let f = Family::Extents;
    let Some((k, val)) = tree::lower_bound(v, root(v, f), f, &extent_key(id, at))? else {
        return Ok(None);
    };
    let (owner, logical) = parse_extent_key(&k)?;
    if owner != id {
        return Ok(None);
    }
    Ok(Some((
        logical,
        Run::decode(&val, v.manifest().blocks, generation(v)?)?,
    )))
}

pub(super) fn covering(v: &impl View, id: u64, logical: u64) -> Result<Option<(u64, Run)>> {
    let f = Family::Extents;
    let Some((k, val)) = tree::floor(v, root(v, f), f, &extent_key(id, logical))? else {
        return Ok(None);
    };
    let (owner, first) = parse_extent_key(&k)?;
    if owner != id {
        return Ok(None);
    }
    let run = Run::decode(&val, v.manifest().blocks, generation(v)?)?;
    if logical < first || logical >= run.end(first)? {
        return Ok(None);
    }
    Ok(Some((first, run)))
}

pub(super) fn checked_page(v: &impl View, r: Ref, logical: u64, size: u64) -> Result<Block> {
    let page = v.checked_data(r)?;
    if logical == size / BLOCK_BYTES
        && !size.is_multiple_of(BLOCK_BYTES)
        && page[(size % BLOCK_BYTES) as usize..]
            .iter()
            .any(|&b| b != 0)
    {
        return Err(Error::Corrupt("suffixe du fichier non nul"));
    }
    Ok(page)
}

fn file(v: &impl View, path: &str) -> Result<Stat> {
    let stat = resolve(v, &components(path)?)?.stat;
    if stat.kind != Kind::File {
        return Err(Error::IsDirectory);
    }
    Ok(stat)
}

impl<D: Device> Engine<D> {
    pub fn extent_count(&self, path: &str) -> Result<usize> {
        self.checked_access(|| {
            let id = file(self, path)?.id;
            let mut cursor = 0;
            let mut count = 0;
            while let Some((logical, run)) = first_extent(self, id, cursor)? {
                cursor = run.end(logical)?;
                count += 1;
            }
            Ok(count)
        })
    }

    pub fn read(&self, path: &str, offset: u64, limit: usize) -> Result<Vec<u8>> {
        self.checked_access(|| {
            if limit > MAX_IO || offset > i64::MAX as u64 {
                return Err(Error::Invalid("plage de lecture"));
            }
            let stat = file(self, path)?;
            let length = (stat.size.saturating_sub(offset)).min(limit as u64) as usize;
            let mut output = Vec::new();
            output
                .try_reserve_exact(length)
                .map_err(|_| Error::NoMemory)?;
            while output.len() < length {
                let pos = offset + output.len() as u64;
                let index = pos / BLOCK_BYTES;
                let mut inside = (pos % BLOCK_BYTES) as usize;
                let Some((first, run)) = covering(self, stat.id, index)? else {
                    let count = (4096 - inside).min(length - output.len());
                    output.resize(output.len() + count, 0);
                    continue;
                };
                // La vue est stable pour cette opération. Chaque page garde sa
                // propre référence et son contrôle d'intégrité, même dans un extent contigu.
                for i in (index - first) as usize..run.hashes.len() {
                    let page = checked_page(self, run.reference(i), first + i as u64, stat.size)?;
                    let count = (4096 - inside).min(length - output.len());
                    output.extend_from_slice(&page[inside..inside + count]);
                    if output.len() == length {
                        break;
                    }
                    inside = 0;
                }
            }
            Ok(output)
        })
    }

    pub fn write(&mut self, path: &str, offset: u64, bytes: &[u8]) -> Result<()> {
        self.prepare_write(path, offset, bytes)?.commit()
    }

    /// Plan entièrement calculé en mémoire, annulable jusqu'à son commit.
    pub fn prepare_write(
        &mut self,
        path: &str,
        offset: u64,
        bytes: &[u8],
    ) -> Result<crate::PreparedWrite<'_, D>> {
        let end = offset
            .checked_add(bytes.len() as u64)
            .filter(|&v| v <= i64::MAX as u64)
            .ok_or(Error::Invalid("dépassement de la taille logique"))?;
        if bytes.len() > MAX_IO {
            return Err(Error::Limit("64 Kio de données par transaction"));
        }
        self.prepare_write_with(|tx| {
            let mut obj = resolve(tx, &components(path)?)?;
            if obj.stat.kind != Kind::File {
                return Err(Error::IsDirectory);
            }
            if bytes.is_empty() {
                return Ok(());
            }
            let new_size = obj.stat.size.max(end);
            tx.resize_logical(obj.stat.size, new_size)?;
            let id = obj.stat.id;
            let first_page = offset / BLOCK_BYTES;
            let after_last = end.div_ceil(BLOCK_BYTES);
            // Le cap de 64 Kio borne à 17 pages, y compris un départ non aligné.
            let mut replaced: Vec<(u64, Run)> = Vec::new();
            let mut fresh = Vec::with_capacity((after_last - first_page) as usize);
            let mut used = 0;
            let mut logical = first_page;
            while logical < after_last {
                let extent = covering(tx, id, logical)?;
                let stop = match &extent {
                    Some((first, run)) => run.end(*first)?.min(after_last),
                    None => logical + 1,
                };
                while logical < stop {
                    let mut page = if let Some((first, run)) = &extent {
                        let old = run.reference((logical - first) as usize);
                        let page = checked_page(tx, old, logical, obj.stat.size)?;
                        tx.retire_block(old.block)?;
                        page
                    } else {
                        [0; 4096]
                    };
                    let inside = if logical == first_page {
                        (offset % BLOCK_BYTES) as usize
                    } else {
                        0
                    };
                    let count = (4096 - inside).min(bytes.len() - used);
                    page[inside..inside + count].copy_from_slice(&bytes[used..used + count]);
                    used += count;
                    fresh.push(tx.alloc_data(&page)?);
                    logical += 1;
                }
                if let Some(extent) = extent {
                    replaced.push(extent);
                }
            }

            let family = Family::Extents;
            for (first, run) in replaced {
                let after = run.end(first)?;
                tx.remove(family, &extent_key(id, first))?;
                if first < first_page {
                    let before = run.slice(0..(first_page - first) as usize);
                    tx.put(family, extent_key(id, first).to_vec(), before.encode())?;
                }
                if after > after_last {
                    let tail = run.slice((after_last - first) as usize..run.hashes.len());
                    tx.put(family, extent_key(id, after_last).to_vec(), tail.encode())?;
                }
            }
            let mut start = 0;
            while start < fresh.len() {
                let head = fresh[start];
                let mut stop = start + 1;
                while stop < fresh.len()
                    && stop - start < MAX_RUN
                    && fresh[stop].birth == head.birth
                    && fresh[stop].block == head.block + (stop - start) as u64
                {
                    stop += 1;
                }
                let run = Run {
                    first: head.block,
                    birth: head.birth,
                    hashes: fresh[start..stop].iter().map(|r| r.hash).collect(),
                };
                tx.put(
                    family,
                    extent_key(id, first_page + start as u64).to_vec(),
                    run.encode(),
                )?;
                start = stop;
            }
            obj.stat.size = new_size;
            obj.stat.revision = generation(tx)?;
            save_object(tx, &obj)
        })
    }
}
