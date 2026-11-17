// Seulement dans le contrôleur SDK de la copie de recette.
#[cfg(feature = "thread-probe")]
pub(super) fn dd_streams() {
    for (words, expected) in [
        (
            &["bs=8", "count=1", "status=noxfer"][..],
            &[0, 1, 2, 3, 4, 5, 6, 7][..],
        ),
        (
            &["ibs=3", "obs=8", "count=8B", "conv=swab", "status=noxfer"],
            &[1, 0, 3, 2, 5, 4, 7, 6],
        ),
        (
            &["bs=8", "skip=2B", "count=4B", "status=noxfer"],
            &[2, 3, 4, 5],
        ),
    ] {
        let (children, [peer]) = group([ALL]);
        let input = peer.boot();
        let output = with(|s| s.ipc.create(peer.0)).unwrap();
        let errors = with(|s| s.ipc.create(peer.0)).unwrap();
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
        peer.send(input, 0, 8, 0);
        assert_eq!(peer.result(), Ok(8));
        peer.recv(output, 15000, false);
        assert_eq!(peer.result(), Ok(1));
        let frame = peer.bytes::<184>(OUTPUT);
        assert_eq!(
            u32::from_le_bytes(frame[28..32].try_into().unwrap()) as usize,
            expected.len()
        );
        assert_eq!(&frame[56..56 + expected.len()], expected);
        peer.recv(errors, 15000, false);
        assert_eq!(peer.result(), Ok(1));
        let frame = peer.bytes::<184>(OUTPUT);
        let n = u32::from_le_bytes(frame[28..32].try_into().unwrap()) as usize;
        let records = core::str::from_utf8(&frame[56..56 + n]).unwrap();
        let want = if words[0] == "ibs=3" {
            "2+1 records in\n1+0 records out\n"
        } else if words.len() == 4 {
            "0+1 records in\n0+1 records out\n"
        } else {
            "1+0 records in\n1+0 records out\n"
        };
        assert_eq!(records, want);
        assert_eq!(child.join(), Outcome::Exit(0));
        stop(children, [peer]);
        hal::log(format_args!(
            "DD-FLUX argv={words:?} octets={expected:?} stderr={records:?} rc=0"
        ));
    }
}
