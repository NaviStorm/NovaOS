//! Trames privées R1. Encodage canonique, réassemblage borné, aucun droit métier.
//! Un échec de transport ne constitue jamais une permission de répéter une mutation.
pub const MAX: usize = 2048;
pub const PAYLOAD: usize = 96;
const MAGIC: &[u8; 8] = b"NXAFRM01";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Direction {
    Request = 1,
    Response = 2,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Format,
    Limit,
    Sequence,
    Incomplete,
    Closed,
}

pub fn frame(
    id: u64,
    direction: Direction,
    data: &[u8],
    offset: usize,
) -> Result<[u8; 128], Error> {
    if id == 0 || data.is_empty() {
        return Err(Error::Format);
    }
    if data.len() > MAX {
        return Err(Error::Limit);
    }
    if offset >= data.len() || offset % PAYLOAD != 0 {
        return Err(Error::Sequence);
    }
    let n = (data.len() - offset).min(PAYLOAD);
    let mut f = [0; 128];
    f[..8].copy_from_slice(MAGIC);
    f[8..16].copy_from_slice(&id.to_le_bytes());
    f[16..18].copy_from_slice(&(data.len() as u16).to_le_bytes());
    f[18..20].copy_from_slice(&(offset as u16).to_le_bytes());
    f[20..22].copy_from_slice(&(n as u16).to_le_bytes());
    f[22] = direction as u8;
    f[32..32 + n].copy_from_slice(&data[offset..offset + n]);
    Ok(f)
}
/// Une boîte par canal, pas de file d'opérations cachée. La première trame
/// consomme son numéro ; abandon/erreur ne permettent pas sa réutilisation.
pub struct Inbox {
    bytes: [u8; MAX],
    last: u64,
    active: Option<(u64, usize, usize, Direction)>,
    closed: bool,
}
impl Inbox {
    pub const fn new() -> Self {
        Self {
            bytes: [0; MAX],
            last: 0,
            active: None,
            closed: false,
        }
    }
    pub fn close(&mut self) {
        self.bytes.fill(0);
        self.active = None;
        self.closed = true;
    }
    pub fn discard(&mut self) {
        self.bytes.fill(0);
        self.active = None;
    }
    pub fn accept(&mut self, packet: &[u8], expected: Direction) -> Result<bool, Error> {
        if self.closed {
            return Err(Error::Closed);
        }
        let result = self.append(packet, expected);
        if result.is_err() {
            self.discard();
        }
        result
    }
    fn append(&mut self, p: &[u8], expected: Direction) -> Result<bool, Error> {
        if p.len() != 128 || &p[..8] != MAGIC || p[22] != expected as u8 || p[23..32] != [0; 9] {
            return Err(Error::Format);
        }
        let id = u64::from_le_bytes(p[8..16].try_into().unwrap());
        let n = |i| u16::from_le_bytes(p[i..i + 2].try_into().unwrap()) as usize;
        let (total, offset, count) = (n(16), n(18), n(20));
        if id == 0 || total == 0 {
            return Err(Error::Format);
        }
        if total > MAX {
            return Err(Error::Limit);
        }
        if offset >= total
            || offset % PAYLOAD != 0
            || count != (total - offset).min(PAYLOAD)
            || p[32 + count..] != [0; 96][..96 - count]
        {
            return Err(Error::Format);
        }
        if let Some((current, len, received, direction)) = self.active {
            if id != current || total != len || offset != received || direction != expected {
                return Err(Error::Sequence);
            }
        } else {
            if offset != 0 || id <= self.last {
                return Err(Error::Sequence);
            }
            self.last = id;
        }
        self.bytes[offset..offset + count].copy_from_slice(&p[32..32 + count]);
        self.active = Some((id, total, offset + count, expected));
        Ok(offset + count == total)
    }
    pub fn message(&self) -> Result<(u64, &[u8]), Error> {
        let (id, n, received, _) = self.active.ok_or(Error::Incomplete)?;
        if n != received {
            return Err(Error::Incomplete);
        }
        Ok((id, &self.bytes[..n]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_boundaries_and_exact_binary_payload() {
        for size in [1, 95, 96, 97, 191, 192, 193, 2048] {
            let data: alloc::vec::Vec<_> = (0..size).map(|i| (i % 256) as u8).collect();
            let mut rx = Inbox::new();
            for at in (0..size).step_by(PAYLOAD) {
                let f = frame(1, Direction::Request, &data, at).unwrap();
                assert_eq!(rx.accept(&f, Direction::Request), Ok(at + 96 >= size));
            }
            assert_eq!(rx.message(), Ok((1, &data[..])));
        }
        assert_eq!(
            frame(1, Direction::Request, &[0; 2049], 0),
            Err(Error::Limit)
        );
    }
    #[test]
    fn lost_response_replay_never_reissues_a_request() {
        let q = frame(4, Direction::Request, b"confirm", 0).unwrap();
        let mut rx = Inbox::new();
        assert_eq!(rx.accept(&q, Direction::Request), Ok(true));
        rx.discard();
        assert_eq!(rx.accept(&q, Direction::Request), Err(Error::Sequence));
        let query = frame(5, Direction::Request, b"outcome", 0).unwrap();
        assert_eq!(rx.accept(&query, Direction::Request), Ok(true));
        rx.close();
        assert_eq!(rx.accept(&query, Direction::Request), Err(Error::Closed));
        assert!(rx.bytes.iter().all(|b| *b == 0));
    }
    #[test]
    fn interleaving_truncation_unknown_versions_and_noncanonical_padding_fail_closed() {
        let mut rx = Inbox::new();
        let data = [7; 97];
        assert_eq!(
            rx.accept(
                &frame(1, Direction::Request, &data, 0).unwrap(),
                Direction::Request
            ),
            Ok(false)
        );
        assert_eq!(
            rx.accept(
                &frame(2, Direction::Request, b"other", 0).unwrap(),
                Direction::Request
            ),
            Err(Error::Sequence)
        );
        assert!(rx.message().is_err());
        assert_eq!(
            rx.accept(
                &frame(1, Direction::Request, &data, 0).unwrap(),
                Direction::Request
            ),
            Err(Error::Sequence)
        );
        for pos in [0, 7, 22, 23, 31, 33, 127] {
            let mut f = frame(3, Direction::Request, b"x", 0).unwrap();
            f[pos] ^= 0x80;
            assert!(Inbox::new().accept(&f, Direction::Request).is_err());
        }
        for size in 0..128 {
            assert!(
                Inbox::new()
                    .accept(&[0; 128][..size], Direction::Request)
                    .is_err()
            );
        }
    }
    #[test]
    fn separate_conversations_cannot_mix_fragments_or_responses() {
        let mut a = Inbox::new();
        let mut b = Inbox::new();
        let data = [3; 193];
        let first = frame(1, Direction::Request, &data, 0).unwrap();
        let second = frame(1, Direction::Request, &data, 96).unwrap();
        assert_eq!(a.accept(&first, Direction::Request), Ok(false));
        assert_eq!(b.accept(&second, Direction::Request), Err(Error::Sequence));
        assert_eq!(a.accept(&second, Direction::Response), Err(Error::Format));
        assert!(a.message().is_err());
        assert!(b.message().is_err());
        assert_eq!(core::mem::size_of::<Inbox>(), 2096);
    }
}
