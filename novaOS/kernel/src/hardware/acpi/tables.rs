//! Lecture des formats filaires ACPI, PCI Firmware (MCFG), Intel VT-d (DMAR)
//! et Arm DEN 0049 (IORT). Les descriptions d'IRQ ou de DMA restent informatives.
use super::*;

pub(super) fn inspect(sig: &[u8; 4], table: &[u8], ecam: &mut Vec<Ecam>) -> Result<()> {
    match sig {
        b"FACP" => {
            let minimum = match table[8] {
                0 => return Err(Error::Format),
                1 => 116,
                2 => 132,
                3 | 4 => 244,
                5 => 268,
                _ => 276,
            };
            if table.len() < minimum {
                return Err(Error::Length);
            }
            if u32_at(table, 40)? == 0 && (table.len() < 148 || u64_at(table, 140)? == 0) {
                return Err(Error::Format);
            }
            // DSDT, FACS et les registres de puissance ne sont pas déréférencés.
            Ok(())
        }
        b"APIC" => subrecords(table, 44, true),
        b"SRAT" => {
            if u32_at(table, 36)? != 1 {
                return Err(Error::Format);
            }
            subrecords(table, 48, false)
        }
        b"SLIT" => {
            let count = u64_at(table, 36)?;
            let distances = table.get(44..).ok_or(Error::Length)?;
            if count == 0 || count.checked_mul(count) != Some(distances.len() as u64) {
                return Err(Error::Length);
            }
            let count = count as usize;
            for (i, row) in distances.chunks_exact(count).enumerate() {
                if row[i] != 10 || row.iter().any(|&distance| distance < 10) {
                    return Err(Error::Format);
                }
            }
            Ok(())
        }
        b"MCFG" => mcfg(table, ecam),
        b"DMAR" => dmar(table),
        b"IORT" => super::iort::inspect(table),
        // Le contenu des tables non interprétées reste opaque après leur checksum.
        _ => Ok(()),
    }
}

fn subrecords(table: &[u8], offset: usize, madt: bool) -> Result<()> {
    let mut tail = table.get(offset..).ok_or(Error::Length)?;
    while !tail.is_empty() {
        let [kind, len] = field(tail, 0)?;
        let minimum = if madt {
            match kind {
                0 => 8,
                1 => 12,
                2 => 10,
                3 => 8,
                4 => 6,
                5 => 12,
                6 => 16,
                7 => 16,
                8 => 16,
                9 => 16,
                10 => 12,
                11 => 76,
                12 | 13 => 24,
                14 => 16,
                15 => 20,
                _ => 2,
            }
        } else {
            match kind {
                0 => 16,
                1 => 40,
                2 => 24,
                3 => 18,
                4 => 12,
                5 | 6 => 32,
                _ => 2,
            }
        };
        if len < minimum || len as usize > tail.len() {
            return Err(Error::Length);
        }
        let (item, rest) = tail.split_at(len as usize);
        if !madt && kind == 1 && u32_at(item, 28)? & 1 != 0 {
            let base = u64_at(item, 8)?;
            let size = u64_at(item, 16)?;
            if size == 0 || base.checked_add(size).is_none() {
                return Err(Error::Address);
            }
        }
        tail = rest;
    }
    Ok(())
}

fn mcfg(table: &[u8], regions: &mut Vec<Ecam>) -> Result<()> {
    let entries = table.get(44..).ok_or(Error::Length)?;
    if entries.is_empty() || !entries.len().is_multiple_of(16) {
        return Err(Error::Length);
    }
    let count = entries.len() / 16;
    if count > MAX_ECAM {
        return Err(Error::Limit);
    }
    regions
        .try_reserve_exact(count)
        .map_err(|_| Error::NoMemory)?;
    for record in entries.chunks_exact(16) {
        let window = Ecam {
            base: u64_at(record, 0)?,
            segment: u16_at(record, 8)?,
            first_bus: record[10],
            last_bus: record[11],
        };
        if window.first_bus > window.last_bus || window.base == 0 || window.base & 0xfffff != 0 {
            return Err(Error::Format);
        }
        let range = window.physical()?;
        for other in regions.iter() {
            if (other.segment == window.segment
                && other.first_bus <= window.last_bus
                && window.first_bus <= other.last_bus)
                || overlaps(&other.physical()?, &range)
            {
                return Err(Error::Overlap);
            }
        }
        regions.push(window);
    }
    Ok(())
}

fn dmar(table: &[u8]) -> Result<()> {
    let mut tail = table.get(48..).ok_or(Error::Length)?;
    while !tail.is_empty() {
        let kind = u16_at(tail, 0)?;
        let len = u16_at(tail, 2)? as usize;
        let (minimum, has_scopes) = match kind {
            0 => (16, true),
            1 => (24, true),
            2 | 5 | 6 => (8, true),
            3 => (20, false),
            4 => (9, false),
            _ => (4, false),
        };
        if len < minimum || len > tail.len() {
            return Err(Error::Length);
        }
        let (item, rest) = tail.split_at(len);
        match kind {
            0 if u64_at(item, 8)? == 0 || u64_at(item, 8)? & 0xfff != 0 => {
                return Err(Error::Address);
            }
            1 if u64_at(item, 8)? > u64_at(item, 16)? => return Err(Error::Address),
            4 if !item[8..].contains(&0) => return Err(Error::Format),
            _ => (),
        }
        if has_scopes {
            let mut scopes = &item[minimum..];
            while !scopes.is_empty() {
                let scope_len = *scopes.get(1).ok_or(Error::Length)? as usize;
                if scope_len < 6 || scope_len > scopes.len() || !scope_len.is_multiple_of(2) {
                    return Err(Error::Length);
                }
                for path in scopes[6..scope_len].chunks_exact(2) {
                    if path[0] > 31 || path[1] > 7 {
                        return Err(Error::Format);
                    }
                }
                scopes = &scopes[scope_len..];
            }
        }
        tail = rest;
    }
    Ok(())
}
