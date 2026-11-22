use super::*;
use std::os::fd::AsRawFd;

fn sector(n: usize, sequence: u64, ack: u64, kind: u8, bytes: &[u8]) -> Vec<u8> {
    let mut data = vec![0; n];
    Cell {
        kind,
        size: bytes.len().try_into().unwrap(),
        link: [0xab; 16],
        sequence,
        ack,
    }
    .encode(&mut data[..HEADER])
    .unwrap();
    data[HEADER..HEADER + bytes.len()].copy_from_slice(bytes);
    data
}
fn route() -> (Link, UnixStream) {
    let (a, b) = UnixStream::pair().unwrap();
    b.set_nonblocking(true).unwrap();
    (Link::new(a).unwrap(), b)
}
#[test]
fn metadata_version_dimensions_et_valeurs_extremes() {
    let empty = State::default();
    let encoded = empty.encode();
    assert_eq!(&encoded[..8], b"NATM\x01\x00\x38\x00");
    assert_eq!(State::decode(&encoded), Ok(empty));
    let full = State {
        link: [0xff; 16],
        tx: u64::MAX,
        rx: u64::MAX,
        ack: u64::MAX - 1,
        sent: 447,
        length: 448,
        received: 448,
        closed: false,
    };
    let encoded = full.encode();
    assert_eq!(State::decode(&encoded), Ok(full));
    for len in [0, 1, 55, 57, 255, 256, 257, 4096] {
        assert!(State::decode(&vec![0; len]).is_err());
    }
    for (index, value) in [
        (0, 0),
        (4, 2),
        (5, 2),
        (6, 57),
        (54, 7),
        (51, 3),
        (48, 255),
        (53, 2),
    ] {
        let mut bad = encoded;
        bad[index] = value;
        assert!(State::decode(&bad).is_err(), "{index}");
    }
    assert_eq!(EXTRA, size_of::<Link>() - 8192);
}
#[test]
fn bornes_des_cellules_et_padding() {
    for n in [512, PAGE] {
        for size in [0, 1, MAX] {
            let kind = if size == 0 { OPEN } else { DATA };
            let mut buf = sector(n, 0x0807060504030201, u64::MAX, kind, &vec![0x7f; size]);
            assert_eq!(&buf[24..32], &[1, 2, 3, 4, 5, 6, 7, 8]);
            let cell = decode_sector(&buf).unwrap();
            let mut header = [0; HEADER];
            cell.encode(&mut header).unwrap();
            assert_eq!(&header[..], &buf[..64]);
            buf[4] += 1;
            assert!(decode_sector(&buf).is_err());
        }
        let mut buf = sector(n, 1, 0, OPEN, &[]);
        buf[n - 1] = 1;
        assert!(decode_sector(&buf).is_err());
        assert!(decode_sector(&buf[..n - 1]).is_err());
        assert!(Cell::decode(&buf[..257]).is_err());
        buf[n - 1] = 0;
        buf[5] = DATA;
        buf[6..8].copy_from_slice(&449u16.to_le_bytes());
        assert!(decode_sector(&buf).is_err());
    }
}
#[test]
fn reponse_perdue_doublon_et_ack_explicite() {
    for n in [512, PAGE] {
        let (mut link, mut peer) = route();
        link.emit(&sector(n, 1, 0, OPEN, &[]));
        let msg = sector(n, 2, 0, DATA, b"opaque");
        link.emit(&msg);
        link.emit(&msg);
        let mut out = [0; 16];
        assert_eq!(peer.read(&mut out).unwrap(), 6);
        assert_eq!(&out[..6], b"opaque");
        assert_eq!(
            peer.read(&mut out).unwrap_err().kind(),
            ErrorKind::WouldBlock
        );
        peer.write_all(b"reponse").unwrap();
        let saved = link.receive(n).to_vec();
        assert_eq!(&saved[64..71], b"reponse");
        assert_eq!(saved, link.receive(n));
        link.emit(&sector(n, 3, 1, ACK, &[]));
        assert_eq!(decode_sector(link.receive(n)).unwrap().kind, EMPTY);
        link.emit(&msg);
        assert_eq!(decode_sector(link.receive(n)).unwrap().kind, BROKEN);
    }
}
#[test]
fn saturation_retrait_et_restitution() {
    let (mut link, mut peer) = route();
    link.emit(&sector(512, 1, 0, OPEN, &[]));
    // Remplir le vrai socket non bloquant, sans allouer de file applicative.
    let fill = [0; 4096];
    let mut total = 0;
    loop {
        match link.relay.write(&fill) {
            Ok(n) => {
                assert!(n > 0);
                total += n
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => break,
            e => panic!("remplissage: {e:?}"),
        }
    }
    let data = sector(512, 2, 0, DATA, &[0x52; MAX]);
    link.emit(&data);
    // Certains systèmes acceptent encore un fragment inférieur à 4 Kio.
    let mut seq = 2;
    while link.state.sent == link.state.length {
        seq += 1;
        assert!(seq < 1024);
        link.emit(&sector(512, seq, 0, DATA, &[0x52; MAX]));
    }
    let held = link.state.tx;
    link.emit(&sector(512, held + 1, 0, DATA, b"suivant"));
    assert_eq!(link.state.tx, held);
    assert!(total > 0);
    link.emit(&sector(512, held + 1, 0, CLOSE, &[]));
    assert!(link.state.closed);
    assert!(link.tx.iter().chain(link.rx.iter()).all(|v| *v == 0));
    let mut buf = [0; 4096];
    while peer.read(&mut buf).unwrap() != 0 {}
    let fd = link.relay.as_raw_fd();
    assert!(fd >= 0);
    drop(link);
    // Une liaison ultérieure a son propre socket et repart sans fragments.
    let (mut next, _other) = route();
    assert_eq!(
        Cell::decode(&next.receive(PAGE)[..HEADER])
            .unwrap()
            .sequence,
        0
    );
}
#[test]
fn coupure_et_refus_des_substitutions() {
    for stage in [0, 1, 2] {
        let (mut link, peer) = route();
        link.emit(&sector(512, 1, 0, OPEN, &[]));
        if stage > 0 {
            link.emit(&sector(512, 2, 0, DATA, b"fixture"));
        }
        if stage > 1 {
            let _ = link.receive(512);
        }
        drop(peer);
        assert_eq!(decode_sector(link.receive(512)).unwrap().kind, BROKEN);
    }
    for index in [4, 8, 24, 32, 40] {
        let (mut link, _peer) = route();
        link.emit(&sector(PAGE, 1, 0, OPEN, &[]));
        let mut next = sector(PAGE, 2, 0, DATA, b"fixture");
        next[index] ^= 1;
        link.emit(&next);
        assert!(link.state.closed);
    }
}

#[test]
fn reouverture_explicite_vide_sans_reprise_ancienne_route() {
    use std::sync::atomic::{AtomicUsize,Ordering};
    static NEXT:AtomicUsize=AtomicUsize::new(0);
    for size in [512,PAGE] {
        let path=std::path::PathBuf::from(std::env::var_os("NOVAOS_TMPDIR").expect("TMPDIR OS"))
            .join(format!("transport-reprise-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
        let listener=UnixListener::bind(&path).unwrap();listener.set_nonblocking(true).unwrap();
        let mut control=Reopener {listener,previous:[0;16]};
        let (mut link,mut old)=route();
        let opening=sector(size,1,0,OPEN,&[]);
        control.open(&mut link,&opening).unwrap();link.emit(&opening);
        link.emit(&sector(size,2,0,DATA,b"ancien"));
        link.emit(&sector(size,3,0,CLOSE,&[]));
        assert!(link.state.closed);
        let mut bytes=[0;32];while old.read(&mut bytes).unwrap()!=0 {}
        // Sans pair nouveau : aucun changement d'état ni attente.
        let mut fresh=opening.clone();fresh[8..24].fill(0xbc);
        control.open(&mut link,&fresh).unwrap();assert!(link.state.closed);
        let mut remote=UnixStream::connect(&path).unwrap();
        remote.set_nonblocking(true).unwrap();
        // Un OPEN ancien ne consomme même pas la connexion en attente.
        control.open(&mut link,&opening).unwrap();assert!(link.state.closed);
        control.open(&mut link,&fresh).unwrap();
        assert!(!link.state.closed);
        assert!(link.tx.iter().chain(link.rx.iter()).all(|&b|b==0));
        link.emit(&fresh);
        assert_eq!(decode_sector(link.receive(size)).unwrap().kind,EMPTY);
        let mut data=sector(size,2,0,DATA,b"neuf");data[8..24].fill(0xbc);
        link.emit(&data);assert_eq!(remote.read(&mut bytes).unwrap(),4);
        assert_eq!(&bytes[..4],b"neuf");
        // Une cellule de l'ancienne route ferme le nouveau transport.
        link.emit(&sector(size,3,0,DATA,b"rejeu"));assert!(link.state.closed);
        drop(control);std::fs::remove_file(path).unwrap();
    }
    assert_eq!(META,56);
    assert!(EXTRA+size_of::<Reopener>()<=256);
}


#[test]
fn page512_nbd_bornes_et_volume_inchange() {
    let path=std::path::PathBuf::from(std::env::var_os("NOVAOS_TMPDIR").unwrap())
        .join(format!("r2-page512-nbd-{}",std::process::id()));
    let file=std::fs::OpenOptions::new().create_new(true).read(true).write(true).open(&path).unwrap();
    file.set_len(VOLUME).unwrap();file.write_all_at(b"temoin P9 intact",0).unwrap();
    let verifier=file.try_clone().unwrap();
    let (server,mut client)=UnixStream::pair().unwrap();
    let (relay,mut authority)=UnixStream::pair().unwrap();
    authority.set_read_timeout(Some(std::time::Duration::from_secs(1))).unwrap();
    let join=std::thread::spawn(move||serve(file,server,relay,512,None).unwrap());
    let mut greeting=[0;152];client.read_exact(&mut greeting).unwrap();
    fn request(c:&mut UnixStream,op:u16,at:u64,n:usize,body:&[u8])->(u32,Vec<u8>) {
        let mut h=[0;28];h[..4].copy_from_slice(&0x25609513u32.to_be_bytes());
        h[6..8].copy_from_slice(&op.to_be_bytes());h[16..24].copy_from_slice(&at.to_be_bytes());h[24..].copy_from_slice(&(n as u32).to_be_bytes());
        c.write_all(&h).unwrap();if op==1 {assert_eq!(body.len(),n);c.write_all(body).unwrap();}
        let mut r=[0;16];c.read_exact(&mut r).unwrap();let status=u32::from_be_bytes(r[4..8].try_into().unwrap());
        let mut out=vec![0;if op==0 && status==0 {n} else {0}];c.read_exact(&mut out).unwrap();(status,out)
    }
    let expected=if cfg!(r2_page_512) {0} else {22};
    assert_eq!(request(&mut client,1,VOLUME,PAGE,&sector(PAGE,1,0,OPEN,&[])).0,expected);
    if cfg!(r2_page_512) {
        assert_eq!(request(&mut client,1,VOLUME,PAGE,&sector(PAGE,2,0,DATA,b"opaque")).0,0);
        let mut seen=[0;6];authority.read_exact(&mut seen).unwrap();assert_eq!(&seen,b"opaque");
        authority.write_all(b"reponse").unwrap();
        let (status,out)=request(&mut client,0,VOLUME+PAGE as u64,PAGE,&[]);assert_eq!(status,0);
        assert_eq!(&out[64..71],b"reponse");assert!(out[512..].iter().all(|&x|x==0));
        let mut bad=sector(PAGE,3,1,ACK,&[]);bad[4095]=1;
        assert_eq!(request(&mut client,1,VOLUME,PAGE,&bad).0,0);
        let (_,out)=request(&mut client,0,VOLUME+PAGE as u64,PAGE,&[]);assert_eq!(decode_sector(&out).unwrap().kind,BROKEN);
    }
    for (op,at,n) in [(0,VOLUME,PAGE),(1,VOLUME+PAGE as u64,PAGE),(0,VOLUME+PAGE as u64,1024),(1,VOLUME,1024),(1,VOLUME+512,PAGE),(0,VOLUME+8192,PAGE)] {
        assert_eq!(request(&mut client,op,at,n,&vec![0;n]).0,22);
    }
    drop(client);join.join().unwrap();
    let mut bytes=[0;16];verifier.read_exact_at(&mut bytes,0).unwrap();assert_eq!(&bytes,b"temoin P9 intact");
    assert_eq!(verifier.metadata().unwrap().len(),VOLUME);drop(verifier);std::fs::remove_file(path).unwrap();
}
