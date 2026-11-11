//! Exécutables ELF64 statiques fournis par le lanceur. Les en-têtes restent immuables.
use super::{Access, DATA, Error, PAGE, Result};

pub(super) const HEAP: u64 = 1 << 46;
pub(crate) const STACK_TOP: u64 = HEAP - 2 * PAGE;
#[derive(Clone, Copy)]
pub(super) struct Elf<'a> {
    headers: &'a [u8],
    pub entry: u64,
    pub stack: u64,
}
#[derive(Clone, Copy)]
pub(super) struct Segment {
    pub start: u64,
    pub end: u64,
    pub offset: usize,
    pub filesz: usize,
    pub access: Access,
}
fn u16_at(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes(b[i..i + 2].try_into().unwrap())
}
fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes(b[i..i + 4].try_into().unwrap())
}
fn u64_at(b: &[u8], i: usize) -> u64 {
    u64::from_le_bytes(b[i..i + 8].try_into().unwrap())
}

impl<'a> Elf<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self> {
        let machine = if cfg!(target_arch = "aarch64") {
            183
        } else {
            62
        };
        if bytes.len() < 64
            || &bytes[..7] != b"\x7fELF\x02\x01\x01"
            || u16_at(bytes, 16) != 2
            || u16_at(bytes, 18) != machine
            || u32_at(bytes, 20) != 1
            || u16_at(bytes, 52) != 64
            || u16_at(bytes, 54) != 56
        {
            return Err(Error::Geometry);
        }
        let start = u64_at(bytes, 32) as usize;
        let end = start
            .checked_add(usize::from(u16_at(bytes, 56)) * 56)
            .ok_or(Error::Geometry)?;
        let headers = bytes.get(start..end).ok_or(Error::Geometry)?;
        let entry = u64_at(bytes, 24);
        let mut stack_size = None;
        let mut executable = false;
        for (i, h) in headers.chunks_exact(56).enumerate() {
            let flags = u32_at(h, 4);
            match u32_at(h, 0) {
                2 | 3 | 7 => return Err(Error::Geometry), // DYNAMIC, INTERP, TLS
                0x6474e551 => {
                    if flags != 6 || stack_size.is_some() {
                        return Err(Error::Geometry);
                    }
                    stack_size = Some(
                        u64_at(h, 40)
                            .max(PAGE)
                            .checked_next_multiple_of(PAGE)
                            .ok_or(Error::Geometry)?,
                    );
                    continue;
                }
                1 => {}
                _ => continue,
            }
            let off = u64_at(h, 8);
            let va = u64_at(h, 16);
            let file = u64_at(h, 32);
            let mem = u64_at(h, 40);
            let align = u64_at(h, 48);
            let end = va.checked_add(mem).ok_or(Error::Geometry)?;
            if !matches!(flags, 4 | 5 | 6)
                || file > mem
                || va < PAGE
                || end > HEAP
                || off.checked_add(file).is_none_or(|n| n > bytes.len() as u64)
                || (align > 1 && (!align.is_power_of_two() || off % align != va % align))
                || va % PAGE != off % PAGE
            {
                return Err(Error::Geometry);
            }
            if mem == 0 {
                continue;
            }
            let pages = (va & !(PAGE - 1))..end.next_multiple_of(PAGE);
            if pages.start < DATA + PAGE && DATA < pages.end {
                return Err(Error::Reserved);
            }
            for earlier in headers[..i * 56]
                .chunks_exact(56)
                .filter(|p| u32_at(p, 0) == 1 && u64_at(p, 40) != 0)
            {
                let first = u64_at(earlier, 16);
                if pages.start < (first + u64_at(earlier, 40)).next_multiple_of(PAGE)
                    && first & !(PAGE - 1) < pages.end
                {
                    return Err(Error::Occupied);
                }
            }
            executable |= flags == 5 && (va..va + file).contains(&entry);
        }
        let stack = STACK_TOP
            .checked_sub(stack_size.unwrap_or(PAGE))
            .filter(|&s| s >= PAGE * 2)
            .ok_or(Error::Geometry)?;
        let elf = Self {
            headers,
            entry,
            stack,
        };
        if !executable
            || (cfg!(target_arch = "aarch64") && entry % 4 != 0)
            || (stack - PAGE < DATA + PAGE && DATA < STACK_TOP + PAGE)
            || elf
                .segments()
                .any(|s| s.start < STACK_TOP + PAGE && stack - PAGE < s.end)
        {
            return Err(Error::Geometry);
        }
        Ok(elf)
    }
    pub fn segments(self) -> impl Iterator<Item = Segment> {
        self.headers
            .chunks_exact(56)
            .filter(|p| u32_at(p, 0) == 1 && u64_at(p, 40) != 0)
            .map(|p| {
                let start = u64_at(p, 16);
                Segment {
                    start,
                    end: start + u64_at(p, 40),
                    offset: u64_at(p, 8) as usize,
                    filesz: u64_at(p, 32) as usize,
                    access: match u32_at(p, 4) {
                        5 => Access::ReadExecute,
                        6 => Access::ReadWrite,
                        _ => Access::ReadOnly,
                    },
                }
            })
    }
}
impl Segment {
    pub fn pages(self) -> impl Iterator<Item = u64> {
        ((self.start & !(PAGE - 1))..self.end).step_by(PAGE as usize)
    }
}
