//! Disposition privée de l'installateur. Tous les intervalles sont demi-ouverts.
use super::{Error, Result, Transport};
use alloc::{vec, vec::Vec};

const ESP: [u8; 16] = [
    0x28, 0x73, 0x2a, 0xc1, 0x1f, 0xf8, 0xd2, 0x11, 0xba, 0x4b, 0x00, 0xa0, 0xc9, 0x3e, 0xc9, 0x3b,
];
// Type réservé par ce prototype, encodage EFI_GUID sur disque.
const P9: [u8; 16] = [
    0x02, 0xf2, 0x87, 0x36, 0x5a, 0x9e, 0xf6, 0x4d, 0x83, 0xdb, 0x17, 0xbb, 0x30, 0x81, 0x72, 0xf9,
];
type Guid = [u8; 16];

pub(super) struct Table {
    pub sector: usize,
    sectors: u64,
    pub esp: u64,
    pub volume: u64,
    pub end: u64,
    ids: [Guid; 3],
}
fn crc(data: &[u8]) -> u32 {
    let mut value = !0u32;
    for b in data {
        value ^= u32::from(*b);
        for _ in 0..8 {
            value = (value >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(value & 1));
        }
    }
    !value
}
impl Table {
    pub fn new(t: &Transport, ids: [Guid; 3]) -> Result<Self> {
        if !matches!(t.size, 512 | 4096)
            || ids
                .iter()
                .enumerate()
                .any(|(i, g)| *g == [0; 16] || ids[..i].contains(g))
        {
            return Err(Error::Geometry);
        }
        let sector = t.size;
        let align = (1 << 20) / sector as u64;
        let esp = align;
        // FAT32 impose au moins 65 525 grappes. L'ESP couvre 60 ou 264 Mio.
        let volume = esp + align * if sector == 512 { 60 } else { 264 };
        let backup = t
            .sectors
            .checked_sub(1 + 16384 / sector as u64)
            .ok_or(Error::Geometry)?;
        let end = backup / align * align;
        let blocks = end.checked_sub(volume).ok_or(Error::Geometry)? / (4096 / sector) as u64;
        if !(512..=1 << 21).contains(&blocks) {
            return Err(Error::Geometry);
        }
        Ok(Self {
            sector,
            sectors: t.sectors,
            esp,
            volume,
            end,
            ids,
        })
    }
    pub fn volume_id(&self) -> Guid {
        self.ids[2]
    }
    fn entries(&self) -> Vec<u8> {
        let mut bytes = vec![0; 16384];
        for (i, (kind, first, end, name)) in [
            (ESP, self.esp, self.volume, "EFI System"),
            (P9, self.volume, self.end, "NovaFS P9"),
        ]
        .iter()
        .enumerate()
        {
            let e = &mut bytes[128 * i..128 * (i + 1)];
            e[..16].copy_from_slice(kind);
            e[16..32].copy_from_slice(&self.ids[i + 1]);
            e[32..40].copy_from_slice(&first.to_le_bytes());
            e[40..48].copy_from_slice(&(end - 1).to_le_bytes());
            for (j, unit) in name.encode_utf16().enumerate() {
                e[56 + 2 * j..58 + 2 * j].copy_from_slice(&unit.to_le_bytes());
            }
        }
        bytes
    }
    fn header(&self, backup: bool, array_crc: u32) -> Vec<u8> {
        let mut h = vec![0; self.sector];
        let last = self.sectors - 1;
        let n = 16384 / self.sector as u64;
        h[..8].copy_from_slice(b"EFI PART");
        h[8..12].copy_from_slice(&0x10000u32.to_le_bytes());
        h[12..16].copy_from_slice(&92u32.to_le_bytes());
        for (o, lba) in [
            (24, if backup { last } else { 1 }),
            (32, if backup { 1 } else { last }),
            (40, 2 + n),
            (48, last - n - 1),
            (72, if backup { last - n } else { 2 }),
        ] {
            h[o..o + 8].copy_from_slice(&lba.to_le_bytes());
        }
        h[56..72].copy_from_slice(&self.ids[0]);
        h[80..84].copy_from_slice(&128u32.to_le_bytes());
        h[84..88].copy_from_slice(&128u32.to_le_bytes());
        h[88..92].copy_from_slice(&array_crc.to_le_bytes());
        let sum = crc(&h[..92]);
        h[16..20].copy_from_slice(&sum.to_le_bytes());
        h
    }
    fn mbr(&self) -> Vec<u8> {
        let mut m = vec![0; self.sector];
        let last = self.sectors - 1;
        let cylinder = last / (255 * 63);
        let chs = if cylinder > 1023 {
            [255; 3]
        } else {
            [
                (last / 63 % 255) as u8,
                (last % 63 + 1) as u8 | ((cylinder >> 2) as u8 & 0xc0),
                cylinder as u8,
            ]
        };
        m[447..451].copy_from_slice(&[0, 2, 0, 0xee]);
        m[451..454].copy_from_slice(&chs);
        m[454..458].copy_from_slice(&1u32.to_le_bytes());
        m[458..462].copy_from_slice(&(last.min(u32::MAX.into()) as u32).to_le_bytes());
        m[510..512].copy_from_slice(&[0x55, 0xaa]);
        m
    }
    pub fn transfer(&self, t: &mut Transport, write: bool) -> Result<()> {
        let entries = self.entries();
        let sum = crc(&entries);
        let mut scratch = vec![0; self.sector];
        let mut region = |t: &mut Transport, first: u64, bytes: &[u8]| -> Result<()> {
            for (i, sector) in bytes.chunks_exact(self.sector).enumerate() {
                let lba = first + i as u64;
                if write {
                    t.p.0[22].mem.put(0, sector);
                    t.output(lba, false)?;
                } else {
                    t.read(lba, &mut scratch)?;
                    if scratch != sector {
                        return Err(Error::Geometry);
                    }
                }
            }
            Ok(())
        };
        for backup in [true, false] {
            let header = if backup { self.sectors - 1 } else { 1 };
            let array = if backup {
                header - 16384 / self.sector as u64
            } else {
                2
            };
            region(t, array, &entries)?;
            region(t, header, &self.header(backup, sum))?;
            if write {
                t.output(0, true)?;
            }
        }
        region(t, 0, &self.mbr())?;
        if write {
            t.output(0, true)?;
        }
        Ok(())
    }
    pub fn open(t: &mut Transport) -> Result<Self> {
        let mut h = vec![0; t.size];
        let mut e = vec![0; t.size];
        t.read(1, &mut h)?;
        t.read(2, &mut e)?;
        let ids = [
            h[56..72].try_into().unwrap(),
            e[16..32].try_into().unwrap(),
            e[144..160].try_into().unwrap(),
        ];
        let table = Self::new(t, ids)?;
        table.transfer(t, false)?;
        Ok(table)
    }
}
