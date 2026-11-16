//! Les acteurs utilisent leurs vrais appels IPC. Le contrôleur observe leur
//! endormissement avant de libérer le pair, sans temporisation heuristique.
use super::*;

pub(super) fn lifecycle() {
    let baseline = heap::audit(false);
    let (children, [owner, writer, reader]) = group([ALL, SEND, RECEIVE]);
    for action in ["donnees", "eof", "retrait", "fermeture", "annulation", "delai"] {
        let (control, tx, rx) = with(|s| {
            let d = &mut s.ipc;
            let control = d.pipe_open(owner.0, false).unwrap();
            let root = d.pipe_check(owner.0, control, None).unwrap();
            let tx = d.pipe_attach(root, writer.0, super::super::pipe::Role::Cat).unwrap();
            assert!(matches!(d.pipe_check(reader.0, control, Some(writer.0)), Err(DENIED)));
            let root = d.pipe_check(owner.0, control, Some(writer.0)).unwrap();
            let rx = d.pipe_attach(root, reader.0, super::super::pipe::Role::Head).unwrap();
            (control, tx, rx)
        });
        assert_eq!(reader.call(IPC_DUP, rx, RECEIVE.into(), 0), Err(DENIED));
        assert_eq!(reader.call(IPC_SEND, rx, DESC, 160), Err(DENIED));
        assert_eq!(writer.call(IPC_RECEIVE, tx, OUTPUT, 184), Err(DENIED));
        reader.give(rx, rx, RECEIVE, 1);
        assert_eq!(reader.result(), Err(DENIED));
        if action == "fermeture" {
            reader.call(IPC_CLOSE, rx, 0, 0).unwrap();
            writer.send(tx, 0, 128, 0);
            assert_eq!(writer.result(), Err(CLOSED));
        } else {
            reader.recv(rx, if action == "delai" { 1 } else { 15000 }, false);
            if action != "delai" { reader.blocked(); }
            match action {
                "donnees" => {
                    writer.send(tx, 0, 128, 0);
                    assert_eq!(writer.result(), Ok(128));
                    assert_eq!(reader.result(), Ok(1));
                    assert_eq!(&reader.bytes::<184>(OUTPUT)[56..], &send_wire(0, 128, 0)[32..]);
                }
                "eof" => {
                    writer.call(IPC_CLOSE, tx, 0, 0).unwrap();
                    assert_eq!(reader.result(), Ok(2));
                    assert_eq!(u64_at(&reader.bytes::<184>(OUTPUT), 48), 1);
                }
                "retrait" => {
                    assert_eq!(owner.call(IPC_REVOKE, control, 0, 0), Ok(2));
                    assert_eq!(reader.result(), Err(REVOKED));
                    reader.recv(rx, 0, false);
                    assert_eq!(reader.result(), Err(DENIED));
                }
                "annulation" => {
                    assert_eq!(owner.call(IPC_CANCEL, control, 0, 0), Ok(1));
                    assert_eq!(reader.result(), Err(CANCELLED));
                }
                "delai" => assert_eq!(reader.result(), Err(TIMED_OUT)),
                _ => unreachable!(),
            }
        }
        owner.call(IPC_CLOSE, control, 0, 0).unwrap();
        hal::log(format_args!("IPC flux {action} OK"));
    }
    let mut controls = [0; 3]; // Le quatrième endpoint du propriétaire sert au bootstrap.
    for control in &mut controls {
        *control = with(|s| s.ipc.pipe_open(owner.0, false)).unwrap();
    }
    assert_eq!(with(|s| s.ipc.pipe_open(owner.0, false)), Err(LIMIT));
    for control in controls {
        owner.call(IPC_CLOSE, control, 0, 0).unwrap();
    }
    stop(children, [owner, writer, reader]);
    assert_eq!(heap::audit(false), baseline);
    hal::log(format_args!("IPC flux quota restitution OK"));
}
