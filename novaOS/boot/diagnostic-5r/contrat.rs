use crate::{Resultat, check, hash, mot, signature};
use alloc::vec::Vec;
use serde::Deserialize;
use sha2::{Digest, Sha256};

type Section<'a> = (usize, usize, &'a str);
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Sections<'a> {
    #[serde(rename = ".text", borrow)]
    text: Section<'a>,
    #[serde(rename = ".eh_fram", borrow)]
    eh: Section<'a>,
    #[serde(rename = ".rdata", borrow)]
    rd: Section<'a>,
    #[serde(rename = ".data", borrow)]
    data: Section<'a>,
    #[serde(rename = ".reloc", borrow)]
    reloc: Section<'a>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Domaines {
    xhci: usize,
    nic: usize,
    stock: usize,
    sonde: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pret<'a> {
    seq: usize,
    role: usize,
    usage: &'a str,
    droits: u64,
    octets: usize,
    pages: usize,
    gpa: u64,
    iova: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Transactions {
    soumises: u64,
    achevees: u64,
}
type Chemin<'a> = (
    (&'a str, u32, u32),
    (&'a str, u8, u8),
    (&'a str, u8, u8),
    (&'a str, u32, u64, u64, &'a str, u8, u8),
);
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Contrat<'a> {
    version: u32,
    epoque: &'a str,
    incarnation: u64,
    #[serde(borrow)]
    source: Chemin<'a>,
    port: u8,
    route: [u8; 0],
    lun: u32,
    rid: usize,
    domaines: Domaines,
    #[serde(borrow)]
    ressources: Vec<Pret<'a>>,
    transactions: Transactions,
    surface: [u64; 6],
    secure_boot: &'a str,
    pe: &'a str,
    esp: &'a str,
    image: &'a str,
    #[serde(borrow)]
    sections: Sections<'a>,
    tables: &'a str,
    proprietaire: &'a str,
    reste: &'a str,
}
fn octets<const N: usize>(s: &str) -> Result<[u8; N], u64> {
    let mut a = [0; N];
    check(s.len() == 2 * N, 55)?;
    fn digit(c: u8) -> Result<u8, u64> {
        match c {
            b'0'..=b'9' => Ok(c - b'0'),
            b'a'..=b'f' => Ok(c - b'a' + 10),
            _ => Err(55),
        }
    }
    for (i, p) in s.as_bytes().chunks_exact(2).enumerate() {
        a[i] = digit(p[0])? * 16 + digit(p[1])?;
    }
    Ok(a)
}
fn chemin(c: &Contrat, raw: &[u8], esp_len: usize) -> Result<(), u64> {
    let (a, p, u, d) = &c.source;
    check(
        raw.len() == 70
            && raw[0..4] == [2, 1, 12, 0]
            && raw[12..16] == [1, 1, 6, 0]
            && raw[18..22] == [3, 5, 6, 0]
            && raw[24..28] == [4, 1, 42, 0]
            && raw[66..] == [127, 255, 4, 0],
        54,
    )?;
    check(
        a.0 == "acpi"
            && matches!(a.1, 0x0a0341d0 | 0x0a0841d0)
            && a.1.to_le_bytes() == raw[4..8]
            && a.2 == 0
            && raw[8..12] == [0; 4]
            && p.0 == "pci"
            && p.1 < 32
            && p.2 < 8
            && raw[16] == p.2
            && raw[17] == p.1
            && usize::from(p.1) * 8 + usize::from(p.2) == c.rid
            && u.0 == "usb"
            && u.1 > 0
            && u.1 == c.port
            && raw[22].checked_add(1) == Some(c.port)
            && raw[23] == 0
            && u.2 == 0
            && d.0 == "partition"
            && d.1 == 1
            && d.1.to_le_bytes() == raw[28..32]
            && d.2 > 0
            && d.2.to_le_bytes() == raw[32..40]
            && d.3.to_le_bytes() == raw[40..48]
            && d.3.checked_mul(512) == Some(esp_len as u64)
            && octets::<16>(d.4)? == raw[48..64]
            && (d.5, d.6) == (2, 2)
            && raw[64..66] == [2, 2]
            && c.lun == 0
            && c.route.is_empty(),
        54,
    )
}
fn pte(page: &mut [u8; 4096], index: usize, value: u64) {
    page[8 * index..8 * index + 8].copy_from_slice(&value.to_le_bytes())
}
// Recalcul de toutes les feuilles et de la racine du profil legacy 5Q. La réserve
// provient des prêts signés et aucune adresse de périphérique n'est déréférencée.
fn traductions(c: &Contrat) -> Result<(u64, u64), u64> {
    let d = &c.domaines;
    let ids = [d.stock, d.nic, d.sonde, d.xhci];
    check(
        d.xhci == c.rid
            && ids
                .iter()
                .enumerate()
                .all(|(i, x)| *x > 0 && *x < 256 && !ids[..i].contains(x)),
        55,
    )?;
    let roles = [
        ("", 0, 0, 0),
        ("DCBAA", 1, 2048, 1),
        ("événements", 2, 8192, 1),
        ("ERST", 1, 256, 1),
        ("commandes/transferts", 1, 4096, 4),
        ("contexte entrée", 1, 4096, 1),
        ("contexte sortie", 3, 4096, 1),
        ("USB IN", 2, 65536, 8),
        ("USB OUT", 1, 4096, 1),
    ];
    check((8..=24).contains(&c.ressources.len()), 55)?;
    let fst = &c.ressources[0];
    let base = fst.gpa.checked_sub(fst.iova).ok_or(55u64)?;
    check(
        base >= 1 << 32 && base % (1 << 21) == 0 && base < (1 << 48) - (1 << 21),
        55,
    )?;
    let mut n = [0; 9];
    let mut struct_page = 32;
    let mut rx = 64;
    let mut tx = 80;
    let mut usb = [0u8; 4096];
    let mut total = 0;
    for (i, l) in c.ressources.iter().enumerate() {
        let &(name, rights, max, limit) = roles.get(l.role).ok_or(55u64)?;
        check(
            l.role > 0
                && l.seq == i + 1
                && l.usage == name
                && l.droits == rights
                && (1..=max).contains(&l.octets)
                && l.pages == l.octets.div_ceil(4096)
                && n[l.role] < limit,
            55,
        )?;
        n[l.role] += 1;
        let (cursor, end) = match l.role {
            7 => (&mut rx, 80),
            8 => (&mut tx, 81),
            _ => (&mut struct_page, 64),
        };
        check(
            *cursor + l.pages <= end
                && (l.role < 7 || l.octets % 4096 == 0)
                && l.iova == (*cursor * 4096) as u64
                && l.gpa == base + l.iova,
            55,
        )?;
        for j in 0..l.pages {
            pte(&mut usb, *cursor + j, (l.gpa + (j * 4096) as u64) | rights);
        }
        *cursor += l.pages;
        total += l.pages;
    }
    check(n[1..].iter().all(|v| *v > 0), 55)?;
    let mut digest = Sha256::new();
    for i in (0..14).chain(20..24) {
        let mut page = [0; 4096];
        let ptr = |p: u64| base + 4096 * p;
        match i {
            0 => pte(&mut page, 0, ptr(1) | 1),
            1 => {
                for (j, id) in ids.iter().enumerate() {
                    pte(&mut page, id * 2, ptr([2, 6, 10, 20][j]) | 1);
                    pte(&mut page, id * 2 + 1, ((j as u64 + 1) << 8) | 2);
                }
            }
            5 => pte(&mut page, 1, ptr(14) | 1),
            9 => pte(&mut page, 1, ptr(15) | 1),
            13 => {
                pte(&mut page, 1, ptr(16) | 1);
                pte(&mut page, 2, ptr(17) | 2);
            }
            23 => page = usb,
            _ => pte(&mut page, 0, ptr(i + 1) | 3),
        }
        digest.update(page);
    }
    check(octets::<32>(c.tables)? == digest.finalize()[..], 55)?;
    Ok((base, total as u64))
}
pub struct Attendu<'a> {
    pub r: &'a Resultat,
    pub esp: &'a [u8],
    pub base: u64,
    pub taille: u64,
    pub offset: usize,
    pub pe: &'a [u8],
    pub surface: [u64; 6],
    pub chemin: &'a [u8],
    pub media: &'a [u8],
}
pub fn verifier(receipt: &[u8], a: &Attendu) -> Result<(u64, u64, u64), u64> {
    check((353..=6144).contains(&receipt.len()), 50)?;
    let b = signature(receipt, 51)?;
    check(&b[..16] == b"NOVA5Q-USB-V1!!!", 50)?;
    check(b[16..32] == a.r.epoque, 52)?;
    let c: Contrat = serde_json::from_slice(&b[288..]).map_err(|_| 54u64)?;
    check(
        c.version == 1
            && octets::<16>(c.epoque)? == a.r.epoque
            && c.incarnation == 1
            && c.secure_boot
                == if a.r.confiance == 1 {
                    "actif"
                } else {
                    "inactif"
                },
        52,
    )?;
    check(
        hash(a.esp) == b[32..64]
            && hash(a.pe) == b[64..96]
            && b[64..96] == a.r.image
            && octets::<32>(c.pe)? == a.r.image
            && octets::<32>(c.esp)? == b[32..64],
        53,
    )?;
    check(
        hash(a.chemin) == b[96..128] && hash(&b[288..]) == b[128..160],
        54,
    )?;
    chemin(&c, a.chemin, a.esp.len())?;
    check(
        c.proprietaire == "firmware Q35 de cette époque"
            && c.reste == "réemploi et export natif interdits"
            && octets::<32>(c.image)? == a.media
            && a.media != [0; 32],
        54,
    )?;
    let t = &c.transactions;
    check(t.soumises > 0 && t.soumises == t.achevees, 56)?;
    let expected = [
        c.rid as u64,
        1,
        a.r.confiance,
        c.port.into(),
        t.soumises,
        t.achevees,
        a.base,
        a.taille,
        a.surface[0],
        a.surface[1],
        a.surface[2],
        a.surface[3],
        a.surface[4],
        a.surface[5],
        a.offset as u64,
        a.pe.len() as u64,
    ];
    check(
        expected
            .iter()
            .enumerate()
            .all(|(i, v)| mot(&b[160 + 8 * i..168 + 8 * i]) == *v)
            && c.surface == a.surface,
        54,
    )?;
    let (reserve, pages) = traductions(&c)?;
    for (addr, size) in [
        (a.esp.as_ptr() as u64, (a.esp.len() + 16384) as u64),
        (a.base, a.taille),
        (a.surface[0], a.surface[1]),
    ] {
        check(
            addr > 0 && addr.checked_add(size).is_some_and(|end| end <= reserve),
            55,
        )?;
    }
    let h = a.pe;
    let pp = u32::from_le_bytes(h.get(60..64).ok_or(53u64)?.try_into().unwrap()) as usize;
    let hdr = h.get(pp..).ok_or(53u64)?;
    check(hdr.len() > 24 && &hdr[..4] == b"PE\0\0", 53)?;
    let n = u16::from_le_bytes(hdr[6..8].try_into().unwrap()) as usize;
    let pos = 24 + u16::from_le_bytes(hdr[20..22].try_into().unwrap()) as usize;
    check(n == 5 && hdr.len() >= pos + 40 * n, 53)?;
    let sections = &c.sections;
    for (name, sect) in [
        (b".text".as_slice(), &sections.text),
        (b".rdata", &sections.rd),
        (b".data", &sections.data),
        (b".eh_fram", &sections.eh),
        (b".reloc", &sections.reloc),
    ] {
        let s = hdr[pos..pos + 40 * n]
            .chunks_exact(40)
            .find(|s| s[..8].split(|b| *b == 0).next() == Some(name))
            .ok_or(53u64)?;
        let sz = u32::from_le_bytes(s[8..12].try_into().unwrap()) as usize;
        let va = u32::from_le_bytes(s[12..16].try_into().unwrap()) as usize;
        check(
            sect.0 == va
                && sect.1 == sz
                && va.checked_add(sz).is_some_and(|v| v <= a.taille as usize),
            53,
        )?;
        let digest = octets::<32>(sect.2)?;
        if name == b".text" {
            let code =
                unsafe { core::slice::from_raw_parts((a.base + va as u64) as *const u8, sz) };
            check(hash(code) == digest, 53)?;
        }
    }
    Ok((c.rid as u64, c.port.into(), pages))
}
