//! Transport utilisateur du protocole fichier NovaOS, partagé avec le SDK.
use crate::runtime::*;

pub fn receive(key: u64) -> Result<ReceiveV1> {
    receive_for(key, TIMEOUT)
}

pub fn receive_for(key: u64, timeout: u64) -> Result<ReceiveV1> {
    receive_options(key, timeout, 0)
}
pub fn receive_options(key: u64, timeout: u64, options: u32) -> Result<ReceiveV1> {
    let mut msg = ReceiveV1 {
        version: VERSION,
        size: 184,
        timeout,
        options,
        reserved: 0,
        kind: 0,
        len: 0,
        tag: 0,
        handle: 0,
        aux: 0,
        payload: [0; 128],
    };
    value(call(IPC_RECEIVE, key, (&raw mut msg) as u64, 184))?;
    Ok(msg)
}

pub fn send(key: u64, tag: u64, bytes: &[u8]) -> Result<()> {
    send_for(key, tag, bytes, TIMEOUT)
}

pub fn send_for(key: u64, tag: u64, bytes: &[u8], timeout: u64) -> Result<()> {
    send_frame(key, tag, bytes, timeout, true)
}

pub(crate) fn send_frame(
    key: u64,
    tag: u64,
    bytes: &[u8],
    timeout: u64,
    rendezvous: bool,
) -> Result<()> {
    if bytes.len() > 128 {
        return Err(BAD_ARGUMENT);
    }
    let mut msg = SendV1 {
        version: VERSION,
        size: 160,
        timeout,
        tag,
        len: bytes.len() as u32,
        flags: u32::from(rendezvous),
        payload: [0; 128],
    };
    msg.payload[..bytes.len()].copy_from_slice(bytes);
    let n = value(call(IPC_SEND, key, (&raw const msg) as u64, 160))?;
    if n == bytes.len() as u64 {
        Ok(())
    } else {
        Err(BAD_ARGUMENT)
    }
}
