use super::cellule::MAX;
pub(super) const META: usize = 56;

#[repr(C)]
#[derive(Default, Debug, PartialEq)]
pub(super) struct State {
    pub(super) link: [u8; 16],
    pub(super) tx: u64,
    pub(super) rx: u64,
    pub(super) ack: u64,
    pub(super) sent: u16,
    pub(super) length: u16,
    pub(super) received: u16,
    pub(super) closed: bool,
}

impl State {
    // Instantané de diagnostic du transport, inutilisable comme jeton de reprise.
    pub(super) fn encode(&self) -> [u8; META] {
        let mut b = [0; META];
        b[..4].copy_from_slice(b"NATM");
        b[4] = 1;
        b[5] = u8::from(self.closed);
        b[6..8].copy_from_slice(&(META as u16).to_le_bytes());
        b[8..24].copy_from_slice(&self.link);
        b[24..32].copy_from_slice(&self.tx.to_le_bytes());
        b[32..40].copy_from_slice(&self.rx.to_le_bytes());
        b[40..48].copy_from_slice(&self.ack.to_le_bytes());
        b[48..50].copy_from_slice(&self.sent.to_le_bytes());
        b[50..52].copy_from_slice(&self.length.to_le_bytes());
        b[52..54].copy_from_slice(&self.received.to_le_bytes());
        b
    }

    pub(super) fn decode(b: &[u8]) -> Result<Self, ()> {
        if b.len() != META
            || &b[..4] != b"NATM"
            || b[4] != 1
            || b[5] > 1
            || b[6..8] != (META as u16).to_le_bytes()
            || b[54..] != [0, 0]
        {
            return Err(());
        }
        let u64_at = |i| u64::from_le_bytes(b[i..i + 8].try_into().unwrap());
        let u16_at = |i| u16::from_le_bytes(b[i..i + 2].try_into().unwrap());
        let state = Self {
            link: b[8..24].try_into().unwrap(),
            tx: u64_at(24),
            rx: u64_at(32),
            ack: u64_at(40),
            sent: u16_at(48),
            length: u16_at(50),
            received: u16_at(52),
            closed: b[5] == 1,
        };
        if state.sent > state.length
            || usize::from(state.length) > MAX
            || usize::from(state.received) > MAX
            || state.rx.checked_sub(state.ack) != Some(u64::from(state.received != 0))
            || (state.tx == 0) != (state.link == [0; 16])
            || (state.tx == 0 && (state.rx != 0 || state.length != 0))
        {
            return Err(());
        }
        Ok(state)
    }
}

const _: () = assert!(META <= 256);
