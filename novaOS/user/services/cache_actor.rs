//! Service de cycle de vie CACHE et acteurs du catalogue de qualification.
//! Les appels de stockage utilisent toujours la capacité propre au processus.
use crate::{protocol as p, runtime as rt};
#[allow(dead_code)]
#[path = "../../abi/files.rs"]
mod fs;
#[path = "cache_steps.rs"]
mod step;
#[allow(dead_code)]
#[path = "../../abi/tmp.rs"]
mod tmp;
#[allow(dead_code)]
#[path = "../../abi/cache.rs"]
mod w;

type Frame = [u8; w::FRAME];
type Token = [u8; 32];

fn control(op: u64, frame: &mut Frame) -> rt::Result<u64> {
    rt::value(rt::call(w::CONTROL, op, 0, frame.as_mut_ptr() as u64))
}

fn require(ok: bool) -> rt::Result<()> {
    if ok { Ok(()) } else { Err(rt::REFUSED) }
}

fn operation(token: &Token, name: &[u8], offset: u64, len: usize) -> rt::Result<Frame> {
    if name.len() > w::DATA - w::NAME || len > w::FRAME - w::DATA {
        return Err(rt::BAD_ARGUMENT);
    }
    let mut frame = [0; w::FRAME];
    frame[w::GEN..w::OP].copy_from_slice(token);
    frame[w::NAME_LEN] = name.len() as u8;
    frame[w::DATA_LEN] = len as u8;
    w::put(&mut frame, w::OFFSET, offset);
    frame[w::NAME..w::NAME + name.len()].copy_from_slice(name);
    Ok(frame)
}

fn read(token: &Token, name: &[u8], destination: &mut [u8]) -> rt::Result<()> {
    let mut frame = operation(token, name, 0, destination.len())?;
    control(w::READ, &mut frame)?;
    require(usize::from(frame[w::DATA_LEN]) == destination.len())?;
    destination.copy_from_slice(&frame[w::DATA..w::DATA + destination.len()]);
    Ok(())
}

fn write(token: &Token, name: &[u8], offset: u64, bytes: &[u8]) -> rt::Result<u64> {
    let mut frame = operation(token, name, offset, bytes.len())?;
    frame[w::DATA..w::DATA + bytes.len()].copy_from_slice(bytes);
    control(w::WRITE, &mut frame)
}

fn private_tmp() -> rt::Result<u64> {
    let cap = rt::value(rt::call(tmp::CAP, 0, 0, 0))?;
    let send = |f: &mut fs::Frame| {
        rt::value(rt::call(
            tmp::CALL,
            cap,
            f.as_mut_ptr() as u64,
            tmp::FRAME as u64,
        ))
    };
    let mut f = fs::frame(fs::OPEN_AT, 1, 0);
    f[2] = fs::CREATE | fs::READABLE | fs::WRITABLE;
    fs::size(&mut f, 3);
    f[fs::PAYLOAD..fs::PAYLOAD + 3].copy_from_slice(b"tmp");
    send(&mut f)?;
    let h = fs::word(&f, 16);
    let mut f = fs::frame(fs::WRITE, 1, h);
    f[2] = fs::START;
    fs::size(&mut f, 1);
    fs::set(&mut f, 32, 1);
    f[fs::PAYLOAD] = 73;
    send(&mut f)?;
    send(&mut fs::frame(fs::SYNC, 1, h))?;
    let mut f = fs::frame(fs::READ, 1, h);
    fs::size(&mut f, 1);
    send(&mut f)?;
    require(fs::length(&f) == 1 && f[fs::PAYLOAD] == 73)?;
    send(&mut fs::frame(fs::CLOSE, 1, h))
}

pub struct Client {
    token: Token,
    other: Token,
    race: Option<rt::Result<u64>>,
}

impl Client {
    pub const fn new() -> Self {
        Self {
            token: [0; 32],
            other: [0; 32],
            race: None,
        }
    }

    pub fn handle(&mut self, id: u64, op: u64, action: u64, arg: u64) -> rt::Result<u64> {
        if id == 10 {
            return match op {
                p::ANNOUNCE => control(w::RECOVER, &mut [0; w::FRAME]),
                p::SAMPLE => Ok(u32::from_le_bytes(*b"CACH") as u64),
                p::MAINTAIN => {
                    match control(w::PRESSURE, &mut [0; w::FRAME]) {
                        Ok(_) => match control(w::AUTO_PURGE, &mut [0; w::FRAME]) {
                            Ok(_) | Err(rt::WOULD_BLOCK | fs::BUSY) => (),
                            Err(e) => return Err(e),
                        },
                        // Un support restauré sans preuve de continuité reste consultable
                        // par le gestionnaire mais interdit toute purge automatique.
                        Err(rt::WOULD_BLOCK | rt::DENIED | fs::BUSY) => (),
                        Err(e) => return Err(e),
                    }
                    Ok(u32::from_le_bytes(*b"CACH") as u64)
                }
                _ => Err(rt::DENIED),
            };
        }
        if !(11..=15).contains(&id) {
            return Err(rt::DENIED);
        }
        if matches!(op, p::ANNOUNCE | p::SAMPLE) {
            return Ok(0);
        }
        if op != p::CACHE_STEP {
            return Err(rt::DENIED);
        }
        match action {
            step::OPEN => {
                let mut f = [0; w::FRAME];
                control(w::OPEN, &mut f)?;
                require(
                    w::get(&f, w::GEN) != 0
                        && w::get(&f, w::CAP) != 0
                        && f[w::BOOT..w::OP].iter().any(|&b| b != 0),
                )?;
                self.token.copy_from_slice(&f[w::GEN..w::OP]);
                Ok(w::get(&f, w::GEN))
            }
            step::READ | step::FOREIGN_READ | step::RACE_READ => {
                let token = if action == step::FOREIGN_READ {
                    &self.other
                } else {
                    &self.token
                };
                let name: &[u8] = if action == step::RACE_READ {
                    b"course"
                } else {
                    b"temoin"
                };
                let mut byte = [0];
                read(token, name, &mut byte)?;
                Ok(byte[0].into())
            }
            step::WRITE | step::FOREIGN_WRITE | step::RACE_WRITE => {
                let token = if action == step::FOREIGN_WRITE {
                    &self.other
                } else {
                    &self.token
                };
                let name: &[u8] = if action == step::RACE_WRITE {
                    b"course"
                } else {
                    b"temoin"
                };
                let outcome = write(
                    token,
                    name,
                    0,
                    &[u8::try_from(arg).map_err(|_| rt::BAD_ARGUMENT)?],
                );
                if action == step::RACE_WRITE {
                    self.race = Some(outcome);
                }
                outcome
            }
            step::RACE_SENT => rt::value(rt::call(w::CONTROL, w::TEST_WRITE_SENT, 0, 0)),
            step::RACE_RESULT => self.race.take().ok_or(rt::WOULD_BLOCK)?,
            step::PURGE | step::PURGE_USER | step::PURGE_SYSTEM => {
                let wire = match action {
                    step::PURGE => w::PURGE,
                    step::PURGE_USER => w::PURGE_USER,
                    _ => w::PURGE_SYSTEM,
                };
                control(wire, &mut operation(&self.token, &[], 0, 0)?)
            }
            step::SAVE_TOKEN => write(&self.token, b"jeton", 0, &self.token),
            step::LOAD_TOKEN => {
                read(&self.token, b"jeton", &mut self.other)?;
                Ok(0)
            }
            step::NAMES => {
                for name in [
                    b"../temoin" as &[u8],
                    b"/documents/exemple.txt",
                    b"a/b",
                    b".",
                    b"..",
                    b"a\0b",
                ] {
                    match write(&self.token, name, 0, &[99]) {
                        Err(rt::DENIED | rt::BAD_ARGUMENT) => (),
                        Err(e) => return Err(e),
                        Ok(_) => return Err(rt::REFUSED),
                    }
                }
                Ok(0)
            }
            step::FORGE => {
                for at in [w::KEY, w::KEY + 16, w::OP, w::STATUS] {
                    let mut f = operation(&self.token, b"temoin", 0, 1)?;
                    f[at] = 1;
                    require(control(w::READ, &mut f) == Err(rt::BAD_ARGUMENT))?;
                }
                for denied in [w::NEXT, w::COMPLETE, w::RECOVER] {
                    require(control(denied, &mut [0; w::FRAME]) == Err(rt::DENIED))?;
                }
                Ok(0)
            }
            step::PAD => {
                for page in 0..16 {
                    write(&self.token, b"reserve", page * 4096, &[99])?;
                }
                Ok(0)
            }
            step::QUOTA => write(&self.token, b"gros", 1024 * 1024, &[1]),
            step::TMP => private_tmp(),
            step::CRASH => crate::node::crash(),
            a if (step::EXPORT..step::EXPORT + 4).contains(&a) => {
                let start = 8 * (a - step::EXPORT) as usize;
                Ok(u64::from_le_bytes(
                    self.token[start..start + 8].try_into().unwrap(),
                ))
            }
            a if (step::IMPORT..step::IMPORT + 4).contains(&a) => {
                let start = 8 * (a - step::IMPORT) as usize;
                self.other[start..start + 8].copy_from_slice(&arg.to_le_bytes());
                Ok(0)
            }
            _ => Err(rt::BAD_ARGUMENT),
        }
    }
}
