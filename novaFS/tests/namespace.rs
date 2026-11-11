//! Les arêtes P8 sont relues sur le support après chaque série de mutations.
use novafs::{Engine, Error, Ids, Kind, Mode, device::SimDevice};

#[test]
fn parcours_par_identite_apres_remontage() {
    let ids = Ids {
        fs: [3; 16],
        lv: [4; 16],
    };
    for sector in [512, 4096] {
        for length in [1023usize, 1024, 1025, 1074, 4096, 8192] {
            let device = SimDevice::new(1024, sector).unwrap();
            let control = device.control();
            let mut fs = Engine::format(device, ids).unwrap();
            let depth = length.div_ceil(256);
            let mut parts = Vec::new();
            let mut parent = fs.root_id();
            for i in 0..depth {
                let edge = length / depth + usize::from(i < length % depth);
                let name = "x".repeat(edge - 1);
                let kind = if i + 1 == depth {
                    Kind::File
                } else {
                    Kind::Directory
                };
                fs.create_at(parent, &name, kind).unwrap();
                parent = fs.lookup_at(parent, &name).unwrap().id;
                parts.push(name);
            }
            let file = parent;
            fs.prepare_write_id(file, 0, b"Nova")
                .unwrap()
                .commit()
                .unwrap();
            let dev = fs.unmount().unwrap();
            let mut fs = Engine::mount(dev, ids, Mode::ReadWrite).unwrap();
            let mut parent = fs.root_id();
            for name in &parts {
                parent = fs.lookup_at(parent, name).unwrap().id;
            }
            assert_eq!(parent, file);
            assert_eq!(fs.read_id(file, 0, 4).unwrap(), b"Nova");
            let root = fs.root_id();
            let gen_before = fs.generation();
            assert!(fs.rename_at(root, &parts[0], root, "../escape").is_err());
            assert_eq!(gen_before, fs.generation());
            if depth == 32 {
                fs.mkdir("spare").unwrap();
                let spare = fs.stat("spare").unwrap().id;
                let generation = fs.generation();
                let image = control.snapshot().bytes().unwrap();
                assert!(matches!(
                    fs.rename_at(root, &parts[0], spare, "moved"),
                    Err(Error::Limit(_))
                ));
                assert_eq!(generation, fs.generation());
                assert_eq!(control.snapshot().bytes().unwrap(), image);
                assert!(matches!(fs.lookup_at(spare, "moved"), Err(Error::NotFound)));
            }
            fs.verify().unwrap();
        }
    }
}
