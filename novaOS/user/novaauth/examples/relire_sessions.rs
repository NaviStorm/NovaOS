//! Relecture indépendante et sans écriture d'un couple P9/autorité du banc de sessions.
use novafs::{Engine, Ids, Mode, device::FileImage};
use novaos_identity::{Authority, Head, Registry, Status, created_user, lab::Ledger};
use std::{env, fs::File, path::Path};
fn main() {
    let args: Vec<_> = env::args().skip(1).collect();
    assert_eq!(args.len(), 3, "dossier secteur valide|refuse");
    let dir = Path::new(&args[0]);
    let sector = args[1].parse().unwrap();
    let disk =
        FileImage::from_file(File::open(dir.join("identites.p9")).unwrap(), 512, sector).unwrap();
    let auth =
        FileImage::from_file(File::open(dir.join("autorite.raw")).unwrap(), 12, sector).unwrap();
    let mut ledger = Ledger::new(auth);
    let result = (|| {
        let h = ledger.current()?;
        let mut fs = Engine::mount_current(
            disk,
            Ids {
                fs: [8; 16],
                lv: [9; 16],
            },
            Mode::ReadOnly,
        )?;
        let mut reg = Registry::open(&mut fs, &mut ledger, [7; 16])?;
        if args[2] == "refuse" {
            return Ok(());
        }
        assert_eq!(h.generation, 7);
        let admin = created_user(&reg.outcome([1; 16])?.unwrap());
        let privileged = reg.grant_attested(admin)?;
        assert!(reg.inspect(privileged, admin)?.admin);
        let new = created_user(&h);
        assert_eq!(reg.inspect(privileged, new)?.name, "alice-b");
        assert_eq!(reg.inspect(privileged, new)?.status, Status::Active);
        let active = reg.grant_attested(new)?;
        assert_eq!(reg.groups(active)?.len(), 1);
        drop(reg);
        let raw = std::fs::read(dir.join("autorite.raw")).unwrap();
        let entry = &raw[2 * 4096..3 * 4096];
        let n = u32::from_le_bytes(entry[40..44].try_into().unwrap()) as usize;
        let prior: Head = serde_json::from_slice(&entry[44..44 + n]).unwrap();
        assert_eq!(prior.generation, 2);
        assert_eq!(ledger.outcome(prior.request)?, Some(prior));
        let old = created_user(&prior);
        assert_ne!(old, new);
        let reg = Registry::open(&mut fs, &mut ledger, [7; 16])?;
        assert_eq!(reg.inspect(privileged, old)?.name, "alice-b");
        assert_eq!(reg.inspect(privileged, old)?.status, Status::Deleted);
        assert!(reg.grant_attested(old).is_err());
        Ok::<(), novaos_identity::Fault>(())
    })();
    match args[2].as_str() {
        "valide" => {
            result.unwrap();
            println!("registre génération 7, tombe et nouvelle identité vérifiées");
        }
        "refuse" => {
            println!("refus vérifié: {:?}", result.unwrap_err());
        }
        _ => panic!("mode inconnu"),
    }
}
