#[cfg(feature = "thread-probe")]
pub(super) fn dd_matrix() {
    fn check(peer: Actor, key: u64, mut expected: &[u8]) {
        while !expected.is_empty() {
            peer.recv(key, 0, false);
            assert_eq!(peer.result(), Ok(1));
            let frame = peer.bytes::<184>(OUTPUT);
            let n = u32::from_le_bytes(frame[28..32].try_into().unwrap()) as usize;
            assert!(n > 0 && n <= expected.len());
            assert_eq!(&frame[56..56 + n], &expected[..n]);
            expected = &expected[n..];
        }
        peer.recv(key, 0, false);
        assert_eq!(peer.result(), Err(WOULD_BLOCK));
    }
    let cases: &[(&[&str], u64, &[u8], &[u8])] = include!("dd-exact.rs");
    let (children, [peer]) = group([ALL]);
    let root = peer.boot();
    for (index, &(words, rc, out, err)) in cases.iter().enumerate() {
        let (input, output, errors) = with(|s| {
            (
                s.ipc.create(peer.0).unwrap(),
                s.ipc.create(peer.0).unwrap(),
                s.ipc.create(peer.0).unwrap(),
            )
        });
        let mut packet = [0u8; 256];
        packet[..8].copy_from_slice(b"NXEXEC01");
        packet[64] = words.len() as u8;
        let mut end = 65;
        for arg in words {
            packet[end..end + 2].copy_from_slice(&(arg.len() as u16).to_le_bytes());
            end += 2;
            packet[end..end + arg.len()].copy_from_slice(arg.as_bytes());
            end += arg.len();
        }
        packet[48..56].copy_from_slice(&((end - 64) as u64).to_le_bytes());
        peer.send(input, 0, 8, 0);
        assert_eq!(peer.result(), Ok(8));
        assert_eq!(peer.call(IPC_NOTIFY, input, 1, 0), Ok(0));
        let child = {
            let _guard = Guard::new();
            let app = spawn::launch_thread_witness(&packet[..end]).unwrap();
            for (index, key, rights) in [(1, input, RECEIVE), (2, output, SEND), (3, errors, SEND)]
            {
                let handle = with(|s| s.ipc.derive(peer.0, key, app.0, rights.into())).unwrap();
                Actor(app.0).put(user::DATA + index * 8, &handle.to_le_bytes());
            }
            app
        };
        hal::log(format_args!("DD-CAS {index} {words:?}"));
        assert_eq!(child.join(), Outcome::Exit(rc));
        check(peer, output, out);
        check(peer, errors, err);
        for key in [input, output, errors] {
            peer.call(IPC_CLOSE, key, 0, 0).unwrap();
        }
        hal::log(format_args!(
            "DD-RESULTAT {index} rc={rc} stdout={} stderr={}",
            out.len(),
            err.len()
        ));
    }
    peer.call(IPC_CLOSE, root, 0, 0).unwrap();
    stop(children, [peer]);
}
