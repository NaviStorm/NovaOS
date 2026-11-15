//! Représentation sur P9 des déclarations v1 de production. Les valeurs variables
//! sont bornées par la déclaration embarquée. Les références Rust ne sont jamais sérialisées.
use super::{
    catalog,
    policy::{Declaration, Restart},
};

pub const COUNT: usize = catalog::INSTALLED.len();
pub const BYTES: usize = 1024;
pub type Records = [Declaration; COUNT];
const MAGIC: &[u8; 8] = b"NVSVCC01";

pub fn defaults() -> Records {
    core::array::from_fn(|i| catalog::INSTALLED[i])
}

fn restart_code(value: Restart) -> u64 {
    match value {
        Restart::Never => 0,
        Restart::Failure => 1,
        Restart::Always => 2,
    }
}

struct Writer<'a>(&'a mut [u8]);
impl Writer<'_> {
    fn bytes(&mut self, bytes: &[u8]) -> Option<()> {
        let (to, rest) = core::mem::take(&mut self.0).split_at_mut_checked(bytes.len())?;
        to.copy_from_slice(bytes);
        self.0 = rest;
        Some(())
    }
    fn word(&mut self, x: u64) -> Option<()> {
        self.bytes(&x.to_le_bytes())
    }
    fn list(&mut self, values: &[u64]) -> Option<()> {
        self.word(values.len() as u64)?;
        for &v in values {
            self.word(v)?;
        }
        Some(())
    }
}

pub fn encode(records: &Records, out: &mut [u8]) -> Option<usize> {
    let size = out.len();
    let mut w = Writer(out);
    w.bytes(MAGIC)?;
    w.word(COUNT as u64)?;
    for d in records {
        for v in [u64::from(d.version), d.id, d.image, d.caps] {
            w.word(v)?;
        }
        for list in [d.args, d.required, d.optional] {
            w.list(list)?;
        }
        for v in [
            u64::from(d.auto),
            restart_code(d.restart),
            d.start_ticks,
            d.stop_ticks,
            d.heartbeat_ticks,
            u64::from(d.retries),
            d.window,
            d.backoff,
            u64::from(d.log),
        ] {
            w.word(v)?;
        }
    }
    Some(size - w.0.len())
}

struct Reader<'a>(&'a [u8]);
impl Reader<'_> {
    fn word(&mut self) -> Option<u64> {
        let (v, tail) = self.0.split_first_chunk::<8>()?;
        self.0 = tail;
        Some(u64::from_le_bytes(*v))
    }
    fn expect(&mut self, value: u64) -> Option<()> {
        (self.word()? == value).then_some(())
    }
    fn list(&mut self, expected: &[u64]) -> Option<()> {
        self.expect(expected.len() as u64)?;
        for &v in expected {
            self.expect(v)?;
        }
        Some(())
    }
    fn flag(&mut self) -> Option<bool> {
        match self.word()? {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }
}

// Les images installées demandent exactement leur masque attesté pour pouvoir
// démarrer. Le catalogue v1 ne contient aucun droit facultatif réductible.
pub fn decode(input: &[u8]) -> Option<Records> {
    let mut r = Reader(input.strip_prefix(MAGIC)?);
    r.expect(COUNT as u64)?;
    let mut out = defaults();
    for d in &mut out {
        r.expect(u64::from(d.version))?;
        r.expect(d.id)?;
        r.expect(d.image)?;
        r.expect(d.caps)?;
        for list in [d.args, d.required, d.optional] {
            r.list(list)?;
        }
        let auto = r.flag()?;
        let restart = match r.word()? {
            0 => Restart::Never,
            n if n == restart_code(d.restart) => d.restart,
            _ => return None,
        };
        let start_ticks = r.word()?;
        let stop_ticks = r.word()?;
        let heartbeat_ticks = r.word()?;
        let retries = u8::try_from(r.word()?).ok()?;
        r.expect(d.window)?;
        r.expect(d.backoff)?;
        r.expect(u64::from(d.log))?;
        if !(1..=d.start_ticks).contains(&start_ticks)
            || !(1..=d.stop_ticks).contains(&stop_ticks)
            || !(1..=d.heartbeat_ticks).contains(&heartbeat_ticks)
            || retries > d.retries
        {
            return None;
        }
        d.auto = auto;
        d.restart = restart;
        d.start_ticks = start_ticks;
        d.stop_ticks = stop_ticks;
        d.heartbeat_ticks = heartbeat_ticks;
        d.retries = retries;
    }
    r.0.is_empty().then_some(out)
}

pub fn differs(a: &Declaration, b: &Declaration) -> bool {
    a.auto != b.auto
        || a.restart != b.restart
        || a.start_ticks != b.start_ticks
        || a.stop_ticks != b.stop_ticks
        || a.heartbeat_ticks != b.heartbeat_ticks
        || a.retries != b.retries
}
pub fn restart(value: Restart) -> u64 {
    restart_code(value)
}
