//! Relit les objets fictifs de la campagne avec le moteur monté en lecture seule.
use novafs::{Engine, Ids, Mode, device::FileImage};
use std::{env, fs::File};
fn main() {
    let args: Vec<_> = env::args().collect();
    let [_, image, sector, fs_id, lv_id, path, size] = args.as_slice() else {
        panic!("arguments du banc")
    };
    let id = |hex: &str| u128::from_str_radix(hex, 16).unwrap().to_be_bytes();
    let disk =
        FileImage::from_file(File::open(image).unwrap(), 512, sector.parse().unwrap()).unwrap();
    let fs = Engine::mount_current(
        disk,
        Ids {
            fs: id(fs_id),
            lv: id(lv_id),
        },
        Mode::ReadOnly,
    )
    .unwrap();
    let size: u64 = size.parse().unwrap();
    let stat = fs.stat(path).unwrap();
    assert_eq!(size, stat.size);
    let mut offset = 0;
    while offset < size {
        let n = (size - offset).min(65536) as usize;
        let bytes = fs.read(path, offset, n).unwrap();
        assert_eq!(bytes.len(), n);
        offset += n as u64;
        let zeros = if offset == size {
            assert_eq!(bytes[n - 1], 0xab);
            n - 1
        } else {
            n
        };
        assert!(bytes[..zeros].iter().all(|&b| b == 0));
    }
    println!("{} {}", stat.id, offset);
}
