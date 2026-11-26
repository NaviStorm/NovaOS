//! Publication bornée du profil Ethernet v1. Les compteurs logiciels conservent
//! le tour complet, tandis que seuls les 16 bits bas franchissent la frontière DMA.
use super::*;

#[derive(Default)]
pub(super) struct Flux {
    tailles: [u32; 8],
    preparees: u32,
    pub(super) publiees: u32,
    pub(super) completees: u32,
    pub(super) en_vol: u16,
    pub(super) rx_lues: u32,
    pub(super) rx_rendues: u16,
    pub(super) rx_base: u32,
}
impl Port<'_> {
    pub(super) fn v1(
        &mut self,
        op: u64,
        len: usize,
        copie: &mut [u8],
    ) -> core::result::Result<usize, u64> {
        if !self.ouvert || self.draine {
            return Err(abi::DENIED);
        }
        let s = &mut self.flux;
        let (tx_avail, tx_used, rx_avail, rx_used) = (
            self.cadres[4].mem,
            self.cadres[5].mem,
            self.cadres[1].mem,
            self.cadres[2].mem,
        );
        match op {
            2 => {
                if !(72..=1526).contains(&len)
                    || len > copie.len()
                    || copie[..12] != [0; 12]
                    || copie[12..18] != [2, 0x4e, 0x56, 0, 0, 1]
                    || copie[18..24] != [0x52, 0x54, 0, 0x12, 0x34, 0x56]
                    || copie[24..26] != [0x88, 0xb5]
                {
                    return Err(abi::BAD_ARGUMENT);
                }
                if s.preparees - s.completees >= 8 {
                    return Err(abi::WOULD_BLOCK);
                }
                let id = (s.preparees as usize) & 15;
                let next = s.preparees.checked_add(1).ok_or(abi::LIMIT)?;
                if s.en_vol & (1 << id) != 0 {
                    return Err(abi::WOULD_BLOCK);
                }
                self.cadres[14 + id / 2]
                    .mem
                    .put((id % 2) * 2048, &copie[..len]);
                self.cadres[3].mem.w32(16 * id + 8, len as u32);
                tx_avail.w16(4 + 2 * id, id as u16);
                s.preparees = next;
                Ok(id)
            }
            32..=39 => {
                let n = (op - 31) as u32;
                if n > s.preparees - s.publiees {
                    return Err(abi::BAD_ARGUMENT);
                }
                let end = s.publiees + n;
                for pos in s.publiees..end {
                    s.en_vol |= 1 << (pos & 15);
                }
                barrier();
                tx_avail.w16(2, end as u16);
                barrier();
                self.kicks[1].w16(0, 1);
                s.tailles[n as usize - 1] += 1;
                s.publiees = end;
                Ok(n as usize)
            }
            7 => {
                let count = tx_used.r16(2).wrapping_sub(s.completees as u16);
                if count == 0 {
                    return Err(abi::WOULD_BLOCK);
                }
                if u32::from(count) > s.publiees - s.completees {
                    return Err(abi::REFUSED);
                }
                barrier();
                let pos = 4 + 8 * ((s.completees as usize) & 15);
                let id = tx_used.r32(pos) as usize;
                if id >= 16 || s.en_vol & (1 << id) == 0 || tx_used.r32(pos + 4) != 0 {
                    return Err(abi::REFUSED);
                }
                s.en_vol &= !(1 << id);
                s.completees += 1;
                Ok(id)
            }
            8 => {
                let count = rx_used.r16(2).wrapping_sub(s.rx_lues as u16);
                if count == 0 {
                    return Err(abi::WOULD_BLOCK);
                }
                if u32::from(count) > 16 - (s.rx_lues - s.rx_base) {
                    return Err(abi::REFUSED);
                }
                barrier();
                let pos = 4 + 8 * ((s.rx_lues as usize) & 15);
                let id = rx_used.r32(pos) as usize;
                let n = rx_used.r32(pos + 4) as usize;
                if id >= 16
                    || s.rx_rendues & (1 << id) != 0
                    || !(72..=1526).contains(&n)
                    || n > copie.len()
                {
                    return Err(abi::REFUSED);
                }
                self.cadres[6 + id / 2]
                    .mem
                    .bytes((id % 2) * 2048, &mut copie[..n]);
                s.rx_rendues |= 1 << id;
                s.rx_lues = s.rx_lues.checked_add(1).ok_or(abi::LIMIT)?;
                Ok(n)
            }
            6 => {
                if s.rx_rendues != u16::MAX || rx_used.r16(2) != s.rx_lues as u16 {
                    return Err(abi::REFUSED);
                }
                let next = s.rx_lues.checked_add(16).ok_or(abi::LIMIT)?;
                for i in 0..16 {
                    self.cadres[6 + i / 2].mem.w16(i % 2 * 2048 + 10, 0x5a5a);
                }
                tx_avail.w16(36, s.completees as u16);
                rx_avail.w16(36, s.rx_lues as u16);
                s.rx_rendues = 0;
                s.rx_base = s.rx_lues;
                barrier();
                rx_avail.w16(2, next as u16);
                barrier();
                self.kicks[0].w16(0, 0);
                Ok(0)
            }
            4 => {
                if s.en_vol != 0
                    || s.preparees != s.completees
                    || s.publiees != s.completees
                    || rx_used.r16(2) != s.rx_lues as u16
                    || tx_used.r16(2) != s.completees as u16
                {
                    return Err(abi::REFUSED);
                }
                if s.rx_rendues != u16::MAX {
                    #[cfg(feature="net-q35-duo")]
                    return Err(abi::REFUSED);
                    #[cfg(not(feature="net-q35-duo"))]
                    {
                        hal::log(format_args!("E3C fermeture RX partielle rx_lus={} tx_lus={}", s.rx_lues, s.completees));
                        return Ok(0);
                    }
                }
                self.draine = true;
                hal::log(format_args!(
                    "ETH3 drainage tx={} rx={} wraps_tx={} wraps_rx={} publications={:?}",
                    s.completees,
                    s.rx_lues,
                    s.completees >> 16,
                    s.rx_lues >> 16,
                    s.tailles
                ));
                Ok(0)
            }
            _ => Err(abi::DENIED),
        }
    }
}
