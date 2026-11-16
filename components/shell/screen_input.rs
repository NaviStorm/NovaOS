use alloc::collections::VecDeque;
use nova_cli::{runtime as rt, tty};
#[derive(Default)]
pub(super) struct Input {
    skip_lf: bool,
    keys: VecDeque<u8>,
    pub(super) active: bool,
    responsive: bool,
    pub(super) size: Option<(u16, u16)>,
}
pub(super) fn screen(action: u64, arg: u64) -> rt::Result<u64> {
    rt::value(rt::call(tty::CONSOLE, tty::SCREEN.into(), action, arg))
}

fn report(bytes: &[u8]) -> Option<(u16, u16)> {
    let fields = bytes.strip_prefix(b"\x1b[")?.strip_suffix(b"R")?;
    let sep = fields.iter().position(|&b| b == b';')?;
    let axis = |s: &[u8]| {
        if s.is_empty() || s.len() > 3 || !s.iter().all(u8::is_ascii_digit) {
            return None;
        }
        core::str::from_utf8(s)
            .ok()?
            .parse::<u16>()
            .ok()
            .filter(|n| *n > 0)
    };
    Some((axis(&fields[..sep])?, axis(&fields[sep + 1..])?))
}
impl Input {
    pub(super) fn leave(&mut self) -> rt::Result<()> {
        if self.active {
            screen(tty::LEAVE_SCREEN, 0)?;
            self.active = false;
        }
        Ok(())
    }
    pub(super) fn geometry(&mut self, refresh: bool) -> rt::Result<(u16, u16)> {
        if !self.active {
            screen(tty::ENTER_SCREEN, 0)?;
            self.active = true;
        }
        if refresh || self.size.is_none() {
            // Les frappes déjà en transit sont rendues dans leur ordre d'origine.
            let mut bytes = [0; 64];
            let mut start = 0;
            let mut len = 0;
            let space = bytes.len().saturating_sub(self.keys.len());
            self.keys.try_reserve(space).map_err(|_| rt::NO_MEMORY)?;
            if space >= 10 {
                self.responsive = false;
                screen(tty::QUERY_SIZE, 0)?;
                while len < space {
                    let b = match screen(tty::READ_TIMED, if len == 0 { 40 } else { 8 }) {
                        Ok(b) => b as u8,
                        Err(rt::TIMED_OUT) => break,
                        Err(e) => return Err(e),
                    };
                    if b == 27 {
                        start = len;
                    }
                    bytes[len] = b;
                    len += 1;
                    if b == b'R'
                        && let Some(dimensions) = report(&bytes[start..len])
                    {
                        self.responsive = true;
                        self.size = Some(dimensions);
                        len = start;
                        break;
                    }
                }
                self.keys.extend(&bytes[..len]);
            }
        }
        Ok(*self.size.get_or_insert((24, 80)))
    }

    pub(super) fn event(&mut self) -> rt::Result<Option<u8>> {
        while self.responsive && self.keys.is_empty() {
            match screen(tty::READ_TIMED, 500) {
                Ok(b) => self.keys.push_back(b as u8),
                Err(rt::TIMED_OUT) => {
                    let previous = self.size;
                    if Some(self.geometry(true)?) != previous {
                        return Ok(None);
                    }
                }
                Err(e) => return Err(e),
            }
        }
        self.key().map(Some)
    }
    pub(super) fn key(&mut self) -> rt::Result<u8> {
        loop {
            let b = match self.keys.pop_front() {
                Some(b) => b,
                None => rt::value(rt::call(tty::CONSOLE, tty::INPUT.into(), 0, 0))? as u8,
            };
            if core::mem::take(&mut self.skip_lf) && b == 10 {
                continue;
            }
            self.skip_lf = b == 13;
            return Ok(b);
        }
    }
}
impl Drop for Input {
    fn drop(&mut self) {
        let _ = self.leave();
    }
}
