//! Qualification en lecture seule de la fenêtre ECAM haute du profil QEMU virt.
//! DTSpec v17, format aplati big endian. Seuls la racine et un hôte PCI enfant
//! direct sont interprétés. Un diagnostic de plateforme ne confère aucun droit.
pub const MAX_BLOB: usize = 2 * 1024 * 1024;
const HEADER: usize = 40;
const MAX_DEPTH: usize = 32;
const VIRT_ECAM: u64 = 0x0040_1000_0000;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Format,
    Limit,
    Platform,
    Missing,
    Ambiguous,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Window {
    pub base: u64,
    pub first_bus: u8,
    pub last_bus: u8,
}
type Result<T> = core::result::Result<T, Error>;

fn be32(bytes: &[u8], offset: usize) -> Result<u32> {
    let value = bytes
        .get(offset..)
        .and_then(|tail| tail.first_chunk::<4>())
        .ok_or(Error::Format)?;
    Ok(u32::from_be_bytes(*value))
}

pub fn declared_size(header: &[u8]) -> Result<usize> {
    if header.len() < HEADER || be32(header, 0)? != 0xd00dfeed {
        return Err(Error::Format);
    }
    let size = be32(header, 4)? as usize;
    if size < HEADER {
        return Err(Error::Format);
    }
    if size > MAX_BLOB {
        return Err(Error::Limit);
    }
    Ok(size)
}

fn split_blocks(blob: &[u8]) -> Result<(&[u8], &[u8])> {
    let size = declared_size(blob)?;
    let blob = blob.get(..size).ok_or(Error::Format)?;
    let structure = be32(blob, 8)? as usize;
    let strings = be32(blob, 12)? as usize;
    let reserved = be32(blob, 16)? as usize;
    let names_len = be32(blob, 32)? as usize;
    let struct_len = be32(blob, 36)? as usize;
    if be32(blob, 20)? < 17
        || be32(blob, 24)? > 17
        || reserved < HEADER
        || reserved % 8 != 0
        || structure < reserved
        || structure > strings
        || strings > size
        || structure % 4 != 0
        || struct_len % 4 != 0
        || struct_len > strings - structure
        || names_len > size - strings
    {
        return Err(Error::Format);
    }
    if !blob[reserved..structure]
        .chunks_exact(16)
        .any(|entry| entry == [0; 16])
    {
        return Err(Error::Format);
    }
    Ok((
        &blob[structure..structure + struct_len],
        &blob[strings..strings + names_len],
    ))
}

fn string(bytes: &[u8], max: usize) -> Result<&[u8]> {
    let len = bytes
        .iter()
        .take(max + 1)
        .position(|&c| c == 0)
        .ok_or(Error::Format)?;
    Ok(&bytes[..len])
}

fn word(bytes: &[u8]) -> Result<u32> {
    if bytes.len() != 4 {
        return Err(Error::Format);
    }
    be32(bytes, 0)
}
fn cells(bytes: &[u8], count: usize) -> Result<u64> {
    if bytes.len() != count * 4 {
        return Err(Error::Format);
    }
    match count {
        1 => Ok(be32(bytes, 0)? as u64),
        2 => Ok((u64::from(be32(bytes, 0)?) << 32) | u64::from(be32(bytes, 4)?)),
        _ => Err(Error::Platform),
    }
}

#[derive(Default)]
struct Node<'a> {
    compatible: Option<&'a [u8]>,
    status: Option<&'a [u8]>,
    address: Option<&'a [u8]>,
    size: Option<&'a [u8]>,
    reg: Option<&'a [u8]>,
    bus: Option<&'a [u8]>,
    domain: Option<&'a [u8]>,
    device_type: Option<&'a [u8]>,
}
impl<'a> Node<'a> {
    fn property(&mut self, name: &[u8], value: &'a [u8]) -> Result<()> {
        let property = match name {
            b"compatible" => &mut self.compatible,
            b"status" => &mut self.status,
            b"#address-cells" => &mut self.address,
            b"#size-cells" => &mut self.size,
            b"reg" => &mut self.reg,
            b"bus-range" => &mut self.bus,
            b"linux,pci-domain" => &mut self.domain,
            b"device_type" => &mut self.device_type,
            _ => return Ok(()),
        };
        if property.replace(value).is_some() {
            return Err(Error::Ambiguous);
        }
        Ok(())
    }
    fn available(&self) -> bool {
        matches!(self.status, None | Some(b"okay\0"))
    }
    fn compatible(&self, expected: &[u8]) -> Result<bool> {
        let Some(list) = self.compatible else {
            return Ok(false);
        };
        let list = list.strip_suffix(&[0]).ok_or(Error::Format)?;
        let mut found = false;
        for item in list.split(|&c| c == 0) {
            if item.is_empty() {
                return Err(Error::Format);
            }
            if item == expected {
                found = true;
            }
        }
        Ok(found)
    }
    fn window(&self, root: &Node) -> Result<Option<Window>> {
        if !self.compatible(b"pci-host-ecam-generic")? || !self.available() {
            return Ok(None);
        }
        if self.compatible != Some(b"pci-host-ecam-generic\0")
            || self.device_type != Some(b"pci\0")
            || self.domain.map(word).transpose()?.unwrap_or(0) != 0
        {
            return Err(Error::Platform);
        }
        let addr_cells = word(root.address.ok_or(Error::Missing)?)? as usize;
        let size_cells = word(root.size.ok_or(Error::Missing)?)? as usize;
        if !(1..=2).contains(&addr_cells) || !(1..=2).contains(&size_cells) {
            return Err(Error::Platform);
        }
        let reg = self.reg.ok_or(Error::Missing)?;
        if reg.len() != (addr_cells + size_cells) * 4 {
            return Err(Error::Format);
        }
        let (a, s) = reg.split_at(addr_cells * 4);
        let base = cells(a, addr_cells)?;
        let size = cells(s, size_cells)?;
        let range = self.bus.ok_or(Error::Missing)?;
        if range.len() != 8 {
            return Err(Error::Format);
        }
        let first = be32(range, 0)?;
        let last = be32(range, 4)?;
        if base != VIRT_ECAM || first != 0 || last > 255 || size != (u64::from(last) + 1) << 20 {
            return Err(Error::Platform);
        }
        Ok(Some(Window {
            base,
            first_bus: 0,
            last_bus: last as u8,
        }))
    }
}

pub fn ecam(blob: &[u8]) -> Result<Window> {
    let (tokens, names) = split_blocks(blob)?;
    let mut cursor = 0;
    let mut depth = 0usize;
    let mut started = false;
    let mut children = [false; MAX_DEPTH];
    let mut root = Node::default();
    let mut child = Node::default();
    let mut window = None;
    loop {
        let token = be32(tokens, cursor)?;
        cursor += 4;
        match token {
            1 => {
                let name = string(&tokens[cursor..], 255)?;
                if depth == 0 {
                    if started || !name.is_empty() {
                        return Err(Error::Format);
                    }
                    started = true;
                } else {
                    if name.is_empty() {
                        return Err(Error::Format);
                    }
                    children[depth - 1] = true;
                }
                if depth == MAX_DEPTH {
                    return Err(Error::Limit);
                }
                children[depth] = false;
                if depth == 1 {
                    child = Node::default();
                }
                depth += 1;
                cursor += (name.len() + 1).next_multiple_of(4);
            }
            2 => {
                if depth == 0 {
                    return Err(Error::Format);
                }
                if depth == 2 {
                    if let Some(found) = child.window(&root)? {
                        if window.replace(found).is_some() {
                            return Err(Error::Ambiguous);
                        }
                    }
                }
                depth -= 1;
            }
            3 => {
                if depth == 0 || children[depth - 1] {
                    return Err(Error::Format);
                }
                let len = be32(tokens, cursor)? as usize;
                let name_at = be32(tokens, cursor + 4)? as usize;
                cursor += 8;
                let name = string(names.get(name_at..).ok_or(Error::Format)?, 63)?;
                if name.is_empty() {
                    return Err(Error::Format);
                }
                let value = tokens
                    .get(cursor..)
                    .and_then(|s| s.get(..len))
                    .ok_or(Error::Format)?;
                match depth {
                    1 => root.property(name, value)?,
                    2 => child.property(name, value)?,
                    _ => (),
                }
                cursor += len.next_multiple_of(4);
            }
            4 => (),
            9 => {
                if cursor != tokens.len() || !started || depth != 0 {
                    return Err(Error::Format);
                }
                if root.compatible != Some(b"linux,dummy-virt\0") || !root.available() {
                    return Err(Error::Platform);
                }
                return window.ok_or(Error::Missing);
            }
            _ => return Err(Error::Format),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Dtb {
        tree: Vec<u8>,
        names: Vec<u8>,
    }
    impl Dtb {
        fn token(&mut self, t: u32) {
            self.tree.extend(t.to_be_bytes());
        }
        fn aligned(&mut self, data: &[u8]) {
            self.tree.extend(data);
            self.tree.resize(self.tree.len().next_multiple_of(4), 0);
        }
        fn begin(&mut self, name: &[u8]) {
            self.token(1);
            self.aligned(name);
        }
        fn prop(&mut self, name: &[u8], value: &[u8]) {
            self.token(3);
            self.token(value.len() as u32);
            self.token(self.names.len() as u32);
            self.names.extend(name);
            self.names.push(0);
            self.aligned(value);
        }
        fn new_root(compat: &[u8]) -> Self {
            let mut d = Self::default();
            d.begin(b"\0");
            d.prop(b"compatible", compat);
            d.prop(b"#address-cells", &2u32.to_be_bytes());
            d.prop(b"#size-cells", &2u32.to_be_bytes());
            d
        }
        fn host(&mut self, status: Option<&[u8]>, base: u64, size: u64, last: u32) {
            self.begin(b"pcie@10000000\0");
            self.prop(b"compatible", b"pci-host-ecam-generic\0");
            self.prop(b"device_type", b"pci\0");
            self.prop(b"linux,pci-domain", &0u32.to_be_bytes());
            if let Some(s) = status {
                self.prop(b"status", s);
            }
            self.prop(b"reg", &[base.to_be_bytes(), size.to_be_bytes()].concat());
            self.prop(
                b"bus-range",
                &[0u32.to_be_bytes(), last.to_be_bytes()].concat(),
            );
            self.token(2);
        }
        fn finish(mut self) -> Vec<u8> {
            self.token(2);
            self.token(9);
            let mut b = vec![0; 56];
            let end = 56 + self.tree.len();
            for (i, v) in [
                0xd00dfeed,
                (end + self.names.len()) as u32,
                56,
                end as u32,
                40,
                17,
                16,
                0,
                self.names.len() as u32,
                self.tree.len() as u32,
            ]
            .iter()
            .enumerate()
            {
                b[i * 4..i * 4 + 4].copy_from_slice(&v.to_be_bytes());
            }
            b.extend(self.tree);
            b.extend(self.names);
            b
        }
    }
    #[test]
    fn racine_virt_et_geometrie_bornee() {
        for (status, last) in [(None, 255), (Some(&b"okay\0"[..]), 7)] {
            let mut d = Dtb::new_root(b"linux,dummy-virt\0");
            d.host(status, VIRT_ECAM, (u64::from(last) + 1) << 20, last);
            assert_eq!(
                ecam(&d.finish()),
                Ok(Window {
                    base: VIRT_ECAM,
                    first_bus: 0,
                    last_bus: last as u8
                })
            );
        }
    }
    #[test]
    fn refuse_plateforme_et_plages_incoherentes() {
        let mut d = Dtb::new_root(b"unknown\0linux,dummy-virt\0");
        d.host(None, VIRT_ECAM, 256 << 20, 255);
        assert_eq!(ecam(&d.finish()), Err(Error::Platform));
        for (base, size, last) in [
            (VIRT_ECAM, 1 << 20, 255),
            (0x40000000, 256 << 20, 255),
            (VIRT_ECAM, 257 << 20, 256),
        ] {
            let mut d = Dtb::new_root(b"linux,dummy-virt\0");
            d.host(None, base, size, last);
            assert_eq!(ecam(&d.finish()), Err(Error::Platform));
        }
    }
    #[test]
    fn ignore_hote_desactive_ou_derriere_bus_traduit() {
        let mut d = Dtb::new_root(b"linux,dummy-virt\0");
        d.host(Some(b"disabled\0"), VIRT_ECAM, 256 << 20, 255);
        d.begin(b"bus\0");
        d.host(None, VIRT_ECAM, 256 << 20, 255);
        d.token(2);
        assert_eq!(ecam(&d.finish()), Err(Error::Missing));
        let mut d = Dtb::new_root(b"linux,dummy-virt\0");
        d.prop(b"status", b"disabled\0");
        d.host(None, VIRT_ECAM, 256 << 20, 255);
        assert_eq!(ecam(&d.finish()), Err(Error::Platform));
    }
    #[test]
    fn refuse_proprietes_et_hotes_ambigus() {
        let mut d = Dtb::new_root(b"linux,dummy-virt\0");
        d.prop(b"#address-cells", &2u32.to_be_bytes());
        assert_eq!(ecam(&d.finish()), Err(Error::Ambiguous));
        let mut d = Dtb::new_root(b"linux,dummy-virt\0");
        d.host(None, VIRT_ECAM, 256 << 20, 255);
        d.host(None, VIRT_ECAM, 256 << 20, 255);
        assert_eq!(ecam(&d.finish()), Err(Error::Ambiguous));
    }
    #[test]
    fn refuse_longueurs_structure_et_profondeur() {
        let mut d = Dtb::new_root(b"linux,dummy-virt\0");
        d.host(None, VIRT_ECAM, 256 << 20, 255);
        let good = d.finish();
        assert!(ecam(&good[..good.len() - 1]).is_err());
        for (idx, value) in [
            (0, 0),
            (4, (MAX_BLOB + 1) as u32),
            (8, 55),
            (12, 55),
            (16, 57),
            (20, 16),
            (24, 18),
            (32, u32::MAX),
            (36, u32::MAX),
        ] {
            let mut t = good.clone();
            t[idx..idx + 4].copy_from_slice(&value.to_be_bytes());
            assert!(ecam(&t).is_err());
        }
        let mut t = good;
        t[40..56].fill(1);
        assert!(ecam(&t).is_err());
        let mut d = Dtb::new_root(b"linux,dummy-virt\0");
        for _ in 0..MAX_DEPTH {
            d.begin(b"dev\0");
        }
        assert_eq!(ecam(&d.finish()), Err(Error::Limit));
    }
}
