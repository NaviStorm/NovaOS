use nova_sha256sum::{Source, digest};
struct Chunks {
    data: Vec<u8>,
    offset: usize,
    strides: Vec<usize>,
    turn: usize,
    fail: bool,
}
impl Source for Chunks {
    type Error = ();
    type Stream = ();
    const BUFFER: usize = 1_000_000;
    fn open(&mut self, _: &[u8]) -> Result<(), ()> {
        Ok(())
    }
    fn read(&mut self, _: &mut (), out: &mut [u8]) -> Result<usize, ()> {
        if self.fail && self.offset > 0 {
            return Err(());
        }
        let n = self.strides[self.turn % self.strides.len()].min(self.data.len() - self.offset);
        out[..n].copy_from_slice(&self.data[self.offset..self.offset + n]);
        self.turn += 1;
        self.offset += n;
        Ok(n)
    }
}
#[test]
fn vectors() {
    for (bytes, expected) in [
        (
            vec![],
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            b"abc".to_vec(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
        (
            vec![b'a'; 1_000_000],
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0",
        ),
    ] {
        for strides in [vec![1_000_000], vec![1], vec![17, 1, 512, 63, 2, 65]] {
            let mut io = Chunks {
                data: bytes.clone(),
                offset: 0,
                strides,
                turn: 0,
                fail: false,
            };
            let hex = digest(&mut io, b"-")
                .unwrap()
                .iter()
                .map(|v| format!("{v:02x}"))
                .collect::<String>();
            assert_eq!(hex, expected);
        }
    }
}
#[test]
fn no_partial_digest() {
    let mut io = Chunks {
        data: b"prefix-suffix".to_vec(),
        offset: 0,
        strides: vec![6],
        turn: 0,
        fail: true,
    };
    assert!(digest(&mut io, b"-").is_err());
}
