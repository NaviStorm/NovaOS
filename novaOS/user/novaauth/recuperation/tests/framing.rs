use novaos_recovery::framing::*;
#[test]
fn frontieres_et_reassemblage() {
    for n in [0, 1, 95, 96, 97, 292, 324, 376, 418, 448, 1024, 2032] {
        let data: Vec<u8> = (0..n).map(|i| i as u8).collect();
        let h = Header {
            sequence: 1,
            operation: 2,
            size: n as u16,
        };
        let mut a = Assembly::new(h).unwrap();
        let mut out = vec![0; n];
        let mut end = false;
        for offset in (0..n.max(1)).step_by(PART) {
            end = a.push(&h.frame(offset, &data).unwrap(), &mut out).unwrap();
        }
        assert!(end);
        assert_eq!(out, data);
        assert!(a.push(&h.frame(0, &data).unwrap(), &mut out).is_err());
    }
}
#[test]
fn mauvais_ordres_tailles_et_reserves() {
    let h = Header {
        sequence: 1,
        operation: 1,
        size: 292,
    };
    let b = [7; 292];
    let first = h.frame(0, &b).unwrap();
    for i in 0..32 {
        let mut bad = first;
        bad[i] ^= 1;
        assert!(
            Assembly::new(h).unwrap().push(&bad, &mut [0; 292]).is_err(),
            "{i}"
        );
    }
    assert!(
        Assembly::new(h)
            .unwrap()
            .push(&h.frame(96, &b).unwrap(), &mut [0; 292])
            .is_err()
    );
    assert!(Header { size: 2033, ..h }.encode().is_err());
    assert!(Header { sequence: 0, ..h }.encode().is_err());
    assert!(h.frame(1, &b).is_err());
    let tail = h.frame(288, &b).unwrap();
    for i in 36..128 {
        let mut a = Assembly::new(h).unwrap();
        let mut out = [0; 292];
        for j in [0, 96, 192] {
            assert!(!a.push(&h.frame(j, &b).unwrap(), &mut out).unwrap());
        }
        let mut bad = tail;
        bad[i] = 1;
        assert!(a.push(&bad, &mut out).is_err());
    }
}
