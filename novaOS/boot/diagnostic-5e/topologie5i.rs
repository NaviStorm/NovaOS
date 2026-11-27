//! Analyse sans accès matériel, partagée entre l’EFI et la contre-lecture hôte.
use alloc::vec::Vec;
pub(crate) const REFUS: u64 = 19;
const MAX_SCOPES: usize = 128;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Rid {
    pub(crate) seg: u16,
    pub(crate) bus: u8,
    pub(crate) dev: u8,
    pub(crate) fun: u8,
}

#[derive(Clone, Copy)]
pub(crate) struct Function {
    pub(crate) id: Rid,
    pub(crate) ident: u32,
    pub(crate) command: u16,
    pub(crate) class: u32,
    pub(crate) header: u8,
    pub(crate) bridge: Option<(u8, u8)>,
}

pub(crate) fn bridge_range(id: Rid, buses: u32) -> Result<(u8, u8), u64> {
    let [primary, secondary, last, _] = buses.to_le_bytes();
    if primary != id.bus || secondary <= primary || secondary > last {
        return Err(REFUS);
    }
    Ok((secondary, last))
}

#[derive(Default)]
pub(crate) struct Dmar<'a> {
    pub(crate) records: Vec<Record<'a>>,
    pub(crate) incomplete: bool,
}
pub(crate) struct Record<'a> {
    pub(crate) segment: u16,
    pub(crate) kind: u16,
    pub(crate) start: u64,
    pub(crate) end: u64,
    pub(crate) all: bool,
    pub(crate) scopes: &'a [u8],
}

// La copie ACPI survit au visiteur grâce au tampon borné de main.
impl<'a> Dmar<'a> {
    pub(crate) fn parse(table: &'a [u8]) -> Result<Self, u64> {
        let mut result = Self::default();
        let mut scopes_count = 0;
        let mut tail = table.get(48..).ok_or(REFUS)?;
        while !tail.is_empty() {
            let head = tail.get(..4).ok_or(REFUS)?;
            let kind = u16::from_le_bytes([head[0], head[1]]);
            let len = usize::from(u16::from_le_bytes([head[2], head[3]]));
            if len < 4 || len > tail.len() {
                return Err(REFUS);
            }
            let (row, rest) = tail.split_at(len);
            tail = rest;
            let min = match kind {
                0 => 16,
                1 => 24,
                _ => {
                    result.incomplete = true;
                    continue;
                }
            };
            let scopes = row.get(min..).ok_or(REFUS)?;
            let seg = u16::from_le_bytes([row[6], row[7]]);
            let addr = u64::from_le_bytes(row[8..16].try_into().unwrap());
            let end = if kind == 1 {
                u64::from_le_bytes(row[16..24].try_into().unwrap())
            } else {
                addr
            };
            let all = kind == 0 && row[4] & 1 != 0;
            if result.records.len() == 32
                || addr & 4095 != 0
                || (kind == 0 && (addr == 0 || row[4] > 1 || row[5] != 0))
                || (kind == 1 && (end < addr || end & 4095 != 4095 || scopes.is_empty()))
            {
                return Err(REFUS);
            }
            if kind == 0
                && result
                    .records
                    .iter()
                    .any(|r| r.kind == 0 && (r.start == addr || (r.all && all && r.segment == seg)))
            {
                return Err(REFUS);
            }
            let mut sc = scopes;
            while !sc.is_empty() {
                let n = *sc.get(1).ok_or(REFUS)? as usize;
                if n < 8 || n > sc.len() || n % 2 != 0 || n > 38 || scopes_count == MAX_SCOPES {
                    return Err(REFUS);
                }
                if sc[6..n].chunks_exact(2).any(|p| p[0] > 31 || p[1] > 7) {
                    return Err(REFUS);
                }
                if !matches!(sc[0], 1..=4) || sc[2..4] != [0, 0] {
                    result.incomplete = true;
                }
                scopes_count += 1;
                sc = &sc[n..];
            }
            result.records.try_reserve(1).map_err(|_| REFUS)?;
            result.records.push(Record {
                segment: seg,
                kind,
                start: addr,
                end,
                all,
                scopes,
            });
        }
        Ok(result)
    }
}

// Une portée irrésolue interdit même l'attribution à l'unité par défaut.
fn matches_scope(segment: u16, scope: &[u8], target: Rid, list: &[Function]) -> Option<bool> {
    if target.seg != segment || matches!(scope[0], 3 | 4) {
        return Some(false);
    }
    if !matches!(scope[0], 1 | 2) {
        return None;
    }
    let mut bus = scope[5];
    let mut path = scope[6..].chunks_exact(2).peekable();
    while let Some(pair) = path.next() {
        let rid = Rid {
            seg: segment,
            bus,
            dev: pair[0],
            fun: pair[1],
        };
        if path.peek().is_none() && scope[0] == 1 {
            if list.iter().any(|f| f.id == rid && f.bridge.is_some()) {
                return None;
            }
            return Some(target == rid);
        }
        let (next, last) = list.iter().find(|f| f.id == rid)?.bridge?;
        if path.peek().is_none() {
            return Some(rid == target || (next..=last).contains(&target.bus));
        }
        bus = next;
    }
    None
}

fn applies(row: &Record<'_>, target: Rid, list: &[Function]) -> Option<bool> {
    let mut sc = row.scopes;
    let mut matched = false;
    while !sc.is_empty() {
        let n = sc[1] as usize;
        matched |= matches_scope(row.segment, &sc[..n], target, list)?;
        sc = &sc[n..];
    }
    Some(matched)
}

pub(crate) fn attribution(
    d: &Dmar<'_>,
    target: Rid,
    list: &[Function],
) -> Option<(Option<u64>, usize)> {
    if d.incomplete {
        return None;
    }
    let (mut explicit, mut fallback, mut rmrr) = (None, None, 0);
    for r in d.records.iter().filter(|r| r.segment == target.seg) {
        let covered = applies(r, target, list)?;
        if r.kind == 1 {
            rmrr += usize::from(covered);
        } else if r.all {
            fallback = Some(r.start);
        } else if covered && explicit.replace(r.start).is_some() {
            return None;
        }
    }
    Some((explicit.or(fallback), rmrr))
}

// Un groupe candidat regroupe les fonctions du même slot et les descendants des
// ponts visibles. Les alias masqués, ACS, ARI et les routes pair-à-pair restent inconnus.
pub(crate) fn possible_group(index: usize, devices: &[Function]) -> (Rid, usize) {
    let mut members = 1u64 << index;
    loop {
        let before = members;
        for (i, a) in devices.iter().enumerate() {
            if members & (1u64 << i) == 0 {
                continue;
            }
            for (j, b) in devices.iter().enumerate() {
                if a.id.seg == b.id.seg
                    && ((a.id.bus == b.id.bus && a.id.dev == b.id.dev)
                        || a.bridge.is_some_and(|(s, e)| (s..=e).contains(&b.id.bus))
                        || b.bridge.is_some_and(|(s, e)| (s..=e).contains(&a.id.bus)))
                {
                    members |= 1u64 << j;
                }
            }
        }
        if before == members {
            break;
        }
    }
    let first = devices
        .iter()
        .enumerate()
        .filter(|(i, _)| members & (1u64 << i) != 0)
        .map(|(_, f)| f.id)
        .min_by_key(|id| (id.bus, id.dev, id.fun))
        .unwrap();
    (first, members.count_ones() as usize)
}

pub(crate) fn validate_bridges(devices: &[Function]) -> Result<(), u64> {
    for (i, a) in devices.iter().enumerate() {
        let Some((s, e)) = a.bridge else {
            continue;
        };
        for b in &devices[i + 1..] {
            let Some((t, u)) = b.bridge else {
                continue;
            };
            if a.id.seg != b.id.seg || e < t || u < s {
                continue;
            }
            if !((s..=e).contains(&b.id.bus) && u <= e || (t..=u).contains(&a.id.bus) && e <= u) {
                return Err(REFUS);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn id(seg: u16, bus: u8, dev: u8, fun: u8) -> Rid {
        Rid { seg, bus, dev, fun }
    }
    fn device(id: Rid, bridge: Option<(u8, u8)>) -> Function {
        Function {
            id,
            bridge,
            ident: 0,
            command: 0,
            class: 0,
            header: 0,
        }
    }
    #[test]
    fn scopes_traversent_les_ponts_sans_changer_de_segment() {
        let b = device(id(4, 10, 2, 0), Some((11, 13)));
        let nic = id(4, 11, 1, 2);
        let path = [1, 10, 0, 0, 0, 10, 2, 0, 1, 2];
        assert_eq!(matches_scope(4, &path, nic, &[b]), Some(true));
        assert_eq!(matches_scope(4, &path, id(5, 11, 1, 2), &[b]), Some(false));
        assert_eq!(matches_scope(4, &path, nic, &[]), None);
        assert_eq!(
            matches_scope(4, &[2, 8, 0, 0, 0, 10, 2, 0], nic, &[b]),
            Some(true)
        );
    }
    #[test]
    fn groupes_candidats_stables_et_ponts_ambigus_refuses() {
        let a = device(id(0, 0, 2, 0), Some((1, 4)));
        let b = device(id(0, 1, 1, 0), Some((2, 3)));
        let nic = device(id(0, 2, 1, 1), None);
        let sibling = device(id(0, 2, 1, 0), None);
        let storage = device(id(0, 0, 8, 0), None);
        let list = [nic, b, storage, sibling, a];
        assert!(validate_bridges(&list).is_ok());
        assert_eq!(possible_group(0, &list), (a.id, 4));
        assert_eq!(possible_group(2, &list), (storage.id, 1));
        assert!(validate_bridges(&[a, device(id(0, 0, 3, 0), Some((2, 5)))]).is_err());
        assert!(bridge_range(a.id, 0).is_err());
    }
    #[test]
    fn priorite_explicite_rmrr_et_ambiguite() {
        let rid = id(8, 10, 2, 5);
        let scope = [1, 8, 0, 0, 0, 10, 2, 5];
        let record = |kind, start, all| Record {
            kind,
            start,
            all,
            segment: 8,
            end: start,
            scopes: &scope,
        };
        let mut d = Dmar {
            records: alloc::vec![
                record(0, 4096, true),
                record(0, 8192, false),
                record(1, 12288, false)
            ],
            incomplete: false,
        };
        assert_eq!(attribution(&d, rid, &[]), Some((Some(8192), 1)));
        d.records.push(record(0, 16384, false));
        assert_eq!(attribution(&d, rid, &[]), None);
        d.incomplete = true;
        assert_eq!(attribution(&d, id(8, 10, 2, 4), &[]), None);
    }
    #[test]
    fn longueurs_adresses_et_budget_refuses() {
        let mut t = alloc::vec![0;48];
        t.extend_from_slice(&[0, 0, 16, 0, 1, 0, 4, 0, 0, 16, 0, 0, 0, 0, 0, 0]);
        assert!(Dmar::parse(&t).is_ok());
        assert!(Dmar::parse(&t[..63]).is_err());
        t.extend_from_within(48..64);
        assert!(Dmar::parse(&t).is_err());
        t.truncate(64);
        t[56] = 1;
        assert!(Dmar::parse(&t).is_err());
        let mut r = alloc::vec![0;48];
        r.extend_from_slice(&[1, 0, 24, 0, 0, 0, 0, 0]);
        r.extend_from_slice(&0x1000u64.to_le_bytes());
        r.extend_from_slice(&0x1fffu64.to_le_bytes());
        assert!(Dmar::parse(&r).is_err());
        let mut unknown = alloc::vec![0;48];
        unknown.extend_from_slice(&[99, 0, 4, 0]);
        assert!(Dmar::parse(&unknown).unwrap().incomplete);
    }
}
