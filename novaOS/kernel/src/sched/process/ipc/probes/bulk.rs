//! Contrat de copie et de retrait exercé par les appels natifs de trois processus.
use super::*;

pub(super) fn contract() {
    let (children, [owner, reader, stranger]) = group([ALL, RECEIVE, ALL]);
    let root = owner.boot();
    let peer = reader.boot();
    let other = stranger.boot();
    with(|s| s.ipc.bind_lane(owner.0, root, reader.0, peer)).unwrap();
    let descriptor = |actor: Actor, tag: u64, address: u64, length: u64| {
        let mut bytes = [0; 32];
        for (chunk, value) in bytes
            .chunks_exact_mut(8)
            .zip([1 | (32 << 32), tag, address, length])
        {
            chunk.copy_from_slice(&value.to_le_bytes());
        }
        actor.put(DESC, &bytes);
    };
    let end = user::DATA + crate::memory::PAGE - 16;
    for (tag, address, length, error) in [
        (0, OUTPUT, 16, BAD_ARGUMENT),
        (1, OUTPUT, 0, BAD_ARGUMENT),
        (1, OUTPUT, 4097, BAD_ARGUMENT),
        (1, u64::MAX - 7, 16, BAD_ADDRESS),
        (1, end, 32, BAD_ADDRESS),
    ] {
        descriptor(owner, tag, address, length);
        assert_eq!(owner.call(IPC_PUT, root, DESC, 32), Err(error));
    }
    let pattern = [0xb7; 32];
    owner.put(OUTPUT, &pattern);
    descriptor(owner, 7, OUTPUT, 32);
    assert_eq!(owner.call(IPC_PUT, root, DESC + 1, 32), Err(BAD_ARGUMENT));
    assert_eq!(owner.call(IPC_PUT, root, DESC, 24), Err(BAD_ARGUMENT));
    assert_eq!(owner.call(IPC_PUT, root, DESC, 32), Ok(32));
    assert_eq!(owner.call(IPC_PUT, root, DESC, 32), Err(WOULD_BLOCK));
    assert_eq!(owner.call(IPC_TAKE, root, DESC, 32), Err(REFUSED));
    assert_eq!(stranger.call(IPC_TAKE, other, DESC, 32), Err(DENIED));
    assert_eq!(reader.call(IPC_PUT, peer, DESC, 32), Err(DENIED));
    reader.put(end, &[0xa5; 16]);
    for (tag, address, length, error) in [
        (8, end, 32, REFUSED),
        (7, OUTPUT, 31, REFUSED),
        (7, user::CODE, 32, BAD_ADDRESS),
        (7, end, 32, BAD_ADDRESS),
    ] {
        descriptor(reader, tag, address, length);
        assert_eq!(reader.call(IPC_TAKE, peer, DESC, 32), Err(error));
    }
    assert_eq!(reader.bytes::<16>(end), [0xa5; 16]);
    reader.memory(|s, m| s.add_second_data(m).unwrap());
    assert_eq!(reader.call(IPC_TAKE, peer, DESC, 32), Ok(32));
    assert_eq!(reader.bytes::<32>(end), pattern);
    assert_eq!(reader.call(IPC_TAKE, peer, DESC, 32), Err(REFUSED));
    assert_eq!(owner.call(IPC_PUT, root, DESC, 32), Ok(32));
    assert_eq!(owner.call(IPC_REVOKE, root, 0, 0), Ok(2));
    assert_eq!(reader.call(IPC_TAKE, peer, DESC, 32), Err(DENIED));
    assert_eq!(owner.call(IPC_PUT, root, DESC, 32), Err(DENIED));
    stop(children, [owner, reader, stranger]);
}
