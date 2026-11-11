//! Temps civil P9. Les générations du journal continuent à ordonner les transactions.
use crate::{Error, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Timestamp {
    seconds: i64,
    nanoseconds: u32,
}

impl Timestamp {
    pub const UNKNOWN: Self = Self {
        seconds: 0,
        nanoseconds: u32::MAX,
    };
    pub fn utc(seconds: i64, nanoseconds: u32) -> Result<Self> {
        if nanoseconds >= 1_000_000_000 {
            return Err(Error::Invalid("fraction de seconde UTC"));
        }
        Ok(Self {
            seconds,
            nanoseconds,
        })
    }
    pub fn parts(self) -> Option<(i64, u32)> {
        (self != Self::UNKNOWN).then_some((self.seconds, self.nanoseconds))
    }
    fn nanos(self) -> Option<i128> {
        self.parts()
            .map(|(s, n)| i128::from(s) * 1_000_000_000 + i128::from(n))
    }
    fn decode(bytes: &[u8]) -> Result<Self> {
        let seconds = i64::from_le_bytes(bytes[..8].try_into().unwrap());
        let nanoseconds = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        let stamp = Self {
            seconds,
            nanoseconds,
        };
        if stamp != Self::UNKNOWN && nanoseconds >= 1_000_000_000 {
            return Err(Error::Corrupt("horodatage P9 non canonique"));
        }
        Ok(stamp)
    }
}

pub trait Clock: Send + Sync {
    /// L'adaptateur peut refuser une horloge indisponible ou manifestement incorrecte.
    fn now(&self) -> Timestamp;
}

pub struct SystemClock;

impl Clock for SystemClock {
    #[cfg(target_os = "none")]
    fn now(&self) -> Timestamp {
        Timestamp::UNKNOWN
    }

    #[cfg(not(target_os = "none"))]
    fn now(&self) -> Timestamp {
        use std::time::{SystemTime, UNIX_EPOCH};
        let n = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(d) => d.as_nanos() as i128,
            Err(e) => -(e.duration().as_nanos() as i128),
        };
        i64::try_from(n.div_euclid(1_000_000_000)).map_or(Timestamp::UNKNOWN, |seconds| Timestamp {
            seconds,
            nanoseconds: n.rem_euclid(1_000_000_000) as u32,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum Atime {
    No = 1,
    #[default]
    Relative = 2,
    Strict = 3,
}

impl Atime {
    pub(crate) fn decode(byte: u8) -> Result<Self> {
        match byte {
            1 => Ok(Self::No),
            2 => Ok(Self::Relative),
            3 => Ok(Self::Strict),
            _ => Err(Error::Corrupt("politique atime P9")),
        }
    }
    pub(crate) fn update(self, t: Times, now: Timestamp) -> bool {
        if self == Self::No || t.atime == now {
            return false;
        }
        let Some(a) = t.atime.nanos() else {
            return true;
        };
        self == Self::Strict
            || [t.mtime, t.ctime]
                .iter()
                .any(|v| v.nanos().is_some_and(|n| a <= n))
            || now.nanos().is_some_and(|n| n - a >= 86_400_000_000_000)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Times {
    pub btime: Timestamp,
    pub mtime: Timestamp,
    pub ctime: Timestamp,
    pub atime: Timestamp,
}

impl Times {
    pub const UNKNOWN: Self = Self::created(Timestamp::UNKNOWN);
    pub const fn created(now: Timestamp) -> Self {
        Self {
            btime: now,
            mtime: now,
            ctime: now,
            atime: now,
        }
    }
    pub fn encode(self) -> [u8; 48] {
        let mut out = [0; 48];
        for (dst, v) in out
            .chunks_exact_mut(12)
            .zip([self.btime, self.mtime, self.ctime, self.atime])
        {
            dst[..8].copy_from_slice(&v.seconds.to_le_bytes());
            dst[8..].copy_from_slice(&v.nanoseconds.to_le_bytes());
        }
        out
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != 48 {
            return Err(Error::Corrupt("taille des horodatages P9"));
        }
        Ok(Self {
            btime: Timestamp::decode(&bytes[..12])?,
            mtime: Timestamp::decode(&bytes[12..24])?,
            ctime: Timestamp::decode(&bytes[24..36])?,
            atime: Timestamp::decode(&bytes[36..48])?,
        })
    }
}
