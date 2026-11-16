use crate::{command::Program, runtime as rt, tty};
use alloc::{string::String, vec, vec::Vec};
pub fn spawn(p: Program, args: &[String], channels: (u64, u64), status: u64) -> rt::Result<u64> {
    launch(p, args, channels, status, p as u64, None)
}
pub fn pipe_spawn(
    p: Program,
    args: &[String],
    channels: (u64, u64),
    status: u64,
    pipe: u64,
) -> rt::Result<u64> {
    let number = match p {
        Program::Cat => tty::PIPE_CAT,
        Program::Wc => tty::PIPE_WC,
        Program::Head => tty::PIPE_HEAD,
        _ => return Err(rt::BAD_ARGUMENT),
    };
    launch(p, args, channels, status, number, Some(pipe))
}
fn launch(
    p: Program,
    args: &[String],
    channels: (u64, u64),
    status: u64,
    number: u64,
    pipe: Option<u64>,
) -> rt::Result<u64> {
    let mut data = vec![0u8; tty::HEADER];
    if args.len() > tty::ARG_COUNT {
        return Err(rt::LIMIT);
    }
    data.push(args.len() as u8);
    for arg in args {
        data.extend_from_slice(
            &u16::try_from(arg.len())
                .map_err(|_| rt::LIMIT)?
                .to_le_bytes(),
        );
        data.extend_from_slice(arg.as_bytes());
    }
    let count = data.len() - tty::HEADER;
    if count > tty::ARG_MAX {
        return Err(rt::LIMIT);
    }
    let reads_input = status == 0
        && channels.0 != 0
        && (p == Program::More
            || matches!(p, Program::Cp | Program::Mv | Program::Rm)
                && crate::options::parse(p, &args.iter().map(String::as_str).collect::<Vec<_>>())?
                    .has(b'i'));
    let streams = tty::ERROR
        | if (status == 0 || p == Program::Head && pipe.is_some() && status == tty::PIPE_PROBE_REVOKE)
            && p != Program::Probe && !(pipe.is_some() && p == Program::Cat) {
            tty::OUTPUT
        } else {
            0
        }
        | if reads_input { tty::INPUT } else { 0 };
    for (i, v) in [
        channels.0,
        channels.1,
        u64::from(streams),
        status,
        count as u64,
    ]
    .into_iter()
    .enumerate()
    {
        data[i * 8..i * 8 + 8].copy_from_slice(&v.to_le_bytes());
    }
    if let Some(key) = pipe {
        data.extend_from_slice(&key.to_le_bytes());
    }
    rt::value(rt::call(
        tty::SPAWN,
        number,
        data.as_ptr() as u64,
        data.len() as u64,
    ))
}
pub const PIPE_FRAME: usize = 128;
const PIPE_TIMEOUT: u64 = 15_000;

/// Zéro désigne uniquement EOF. Une révocation ou une échéance reste une erreur.
pub fn pipe_read(key: u64, dest: &mut [u8]) -> rt::Result<usize> {
    if dest.len() < PIPE_FRAME {
        return Err(rt::BAD_ARGUMENT);
    }
    let frame = rt::receive_for(key, PIPE_TIMEOUT)?;
    match (frame.kind, frame.tag, frame.len, frame.handle, frame.aux) {
        (2, 0, 0, 0, 1) => Ok(0),
        (1, 0, 1..=128, 0, 0) => {
            let len = frame.len as usize;
            dest[..len].copy_from_slice(&frame.payload[..len]);
            Ok(len)
        }
        _ => Err(rt::BAD_ARGUMENT),
    }
}

/// En cas d'erreur, le préfixe admis reste acquis et ne doit pas être rejoué.
pub fn pipe_write(key: u64, bytes: &[u8]) -> Result<usize, (usize, u64)> {
    let mut admitted = 0;
    for part in bytes.chunks(PIPE_FRAME) {
        let result = match rt::send_frame(key, 0, part, 0, false) {
            Err(rt::WOULD_BLOCK) => rt::send_for(key, 0, part, PIPE_TIMEOUT),
            other => other,
        };
        result.map_err(|code| (admitted, code))?;
        admitted += part.len();
    }
    Ok(admitted)
}
pub fn wait(key: u64) -> rt::Result<(bool, u64)> {
    let mut info = rt::WaitInfo {
        version: 0,
        size: 0,
        process: 0,
        kind: 0,
        code: 0,
    };
    rt::value(rt::call(rt::PROCESS_WAIT, key, (&raw mut info) as u64, 32))?;
    Ok((info.kind != 0, info.code))
}
pub fn arguments() -> rt::Result<Vec<&'static str>> {
    let n = usize::try_from(rt::config(4)).map_err(|_| rt::BAD_ARGUMENT)?;
    if !(1..=tty::ARG_MAX).contains(&n) {
        return Err(rt::BAD_ARGUMENT);
    }
    // Sûreté : page DATA privée mappée pour toute la vie de l'ELF. Cette zone
    // d'arguments est initialisée avant publication et ce runtime ne la modifie jamais.
    let raw = unsafe { core::slice::from_raw_parts((0x401000 + tty::HEADER) as *const u8, n) };
    let mut rest = &raw[1..];
    let mut args = Vec::new();
    if raw[0] as usize > tty::ARG_COUNT {
        return Err(rt::BAD_ARGUMENT);
    }
    for _ in 0..raw[0] {
        let (length, more) = rest.split_at_checked(2).ok_or(rt::BAD_ARGUMENT)?;
        let (s, tail) = more
            .split_at_checked(u16::from_le_bytes(length.try_into().unwrap()) as usize)
            .ok_or(rt::BAD_ARGUMENT)?;
        args.push(core::str::from_utf8(s).map_err(|_| rt::BAD_ARGUMENT)?);
        rest = tail;
    }
    if !rest.is_empty() {
        return Err(rt::BAD_ARGUMENT);
    }
    Ok(args)
}
