//! Arm DEN 0049. Contrôle de la forme du graphe, sans création de domaine.
use super::*;

const MAX_NODES: usize = 256;

fn array(
    offset: usize,
    count: usize,
    stride: usize,
    header: usize,
    limit: usize,
) -> Result<Range<usize>> {
    if count == 0 {
        return Ok(0..0);
    }
    if offset < header || offset > limit || count > (limit - offset) / stride {
        return Err(Error::Length);
    }
    Ok(offset..offset + count * stride)
}

fn conflict(a: &Range<usize>, b: &Range<usize>) -> bool {
    !a.is_empty() && !b.is_empty() && a.start < b.end && b.start < a.end
}

fn mappings(node: &[u8]) -> Result<&[u8]> {
    let count = u32_at(node, 8)? as usize;
    let offset = u32_at(node, 12)? as usize;
    let minimum = match (node[0], node[3]) {
        (0, _) => 20,
        (1, _) => 30,
        (2, 0) => 32,
        (2, _) => 36,
        (3, _) => 60,
        (4, 0) => 60,
        (4, _) => 68,
        (5, 0) => 32,
        (5, _) => 40,
        (6, _) => 28,
        (7, _) => 27,
        _ => 16,
    };
    if node.len() < minimum {
        return Err(Error::Length);
    }
    let maps = array(offset, count, 20, minimum, node.len())?;
    match node[0] {
        0 => {
            let ids = u32_at(node, 16)? as usize;
            if ids == 0 || count != 0 {
                return Err(Error::Format);
            }
            array(20, ids, 4, 20, node.len())?;
        }
        1 => {
            let end = if count == 0 { node.len() } else { maps.start };
            if !node[29..end].contains(&0) {
                return Err(Error::Format);
            }
        }
        3 => {
            let global = array(u32_at(node, 40)? as usize, 2, 8, minimum, node.len())?;
            let context = array(
                u32_at(node, 48)? as usize,
                u32_at(node, 44)? as usize,
                8,
                minimum,
                node.len(),
            )?;
            let pmu = array(
                u32_at(node, 56)? as usize,
                u32_at(node, 52)? as usize,
                8,
                minimum,
                node.len(),
            )?;
            if conflict(&global, &context)
                || conflict(&global, &pmu)
                || conflict(&context, &pmu)
                || [&global, &context, &pmu]
                    .iter()
                    .any(|range| conflict(range, &maps))
            {
                return Err(Error::Overlap);
            }
        }
        6 => {
            let ranges = array(
                u32_at(node, 24)? as usize,
                u32_at(node, 20)? as usize,
                20,
                minimum,
                node.len(),
            )?;
            if ranges.is_empty() || conflict(&ranges, &maps) {
                return Err(Error::Format);
            }
            let descriptors = &node[ranges];
            let mut physical = Vec::new();
            physical
                .try_reserve_exact(descriptors.len() / 20)
                .map_err(|_| Error::NoMemory)?;
            for descriptor in descriptors.chunks_exact(20) {
                let base = u64_at(descriptor, 0)?;
                let size = u64_at(descriptor, 8)?;
                if size == 0 || (base | size) & 0xffff != 0 {
                    return Err(Error::Address);
                }
                physical.push(base..base.checked_add(size).ok_or(Error::Address)?);
            }
            physical.sort_unstable_by_key(|r| r.start);
            if physical.windows(2).any(|pair| overlaps(&pair[0], &pair[1])) {
                return Err(Error::Overlap);
            }
        }
        7 => {
            let end = if count == 0 { node.len() } else { maps.start };
            if count != 1 || !node[26..end].contains(&0) {
                return Err(Error::Format);
            }
        }
        _ => (),
    }
    Ok(&node[maps])
}

pub(super) fn inspect(table: &[u8]) -> Result<()> {
    let count = u32_at(table, 36)? as usize;
    let mut offset = u32_at(table, 40)? as usize;
    if count > MAX_NODES {
        return Err(Error::Limit);
    }
    if offset < 48 || offset > table.len() {
        return Err(Error::Length);
    }
    let mut nodes = Vec::new();
    nodes
        .try_reserve_exact(count)
        .map_err(|_| Error::NoMemory)?;
    for _ in 0..count {
        let len = u16_at(&table[offset..], 1)? as usize;
        if len < 16 || len > table.len() - offset {
            return Err(Error::Length);
        }
        let node = &table[offset..offset + len];
        nodes.push((offset, node, mappings(node)?));
        offset += len;
    }
    if offset != table.len() {
        return Err(Error::Length);
    }
    let target_index = |reference| {
        nodes
            .binary_search_by_key(&reference, |n| n.0)
            .map_err(|_| Error::Format)
    };
    let mut incoming = [0u16; MAX_NODES];
    for &(start, node, maps) in &nodes {
        if node[0] == 5 {
            let target = target_index(u32_at(node, 28)? as usize)?;
            if !matches!(nodes[target].1[0], 1 | 2 | 4) {
                return Err(Error::Format);
            }
        }
        for map in maps.chunks_exact(20) {
            let target = u32_at(map, 12)? as usize;
            if target == start {
                return Err(Error::Format);
            }
            let index = target_index(target)?;
            incoming[index] = incoming[index].checked_add(1).ok_or(Error::Limit)?;
        }
    }
    // Tri topologique itératif. Chaque lien est traité deux fois, avec un
    // index borné des offsets. Une chaîne longue n'utilise pas la pile d'appel.
    let mut ready = Vec::new();
    ready
        .try_reserve_exact(count)
        .map_err(|_| Error::NoMemory)?;
    for (index, &degree) in incoming[..count].iter().enumerate() {
        if degree == 0 {
            ready.push(index);
        }
    }
    let mut visited = 0;
    while let Some(index) = ready.pop() {
        visited += 1;
        for map in nodes[index].2.chunks_exact(20) {
            let target = target_index(u32_at(map, 12)? as usize)?;
            incoming[target] -= 1;
            if incoming[target] == 0 {
                ready.push(target);
            }
        }
    }
    if visited != count {
        return Err(Error::Format);
    }
    Ok(())
}
