use novafs::{Device, Engine, Ids, Mode, device::SimDevice};
use novaos_identity::{lab::Ledger, *};

const IDS: Ids = Ids {
    fs: [31; 16],
    lv: [32; 16],
};
const DOMAIN: Id = [41; 16];

struct LoseAck {
    provider: Ledger<SimDevice>,
    lose: bool,
}
impl Authority for LoseAck {
    fn current(&self) -> Result<Head> {
        self.provider.current()
    }
    fn outcome(&self, r: Id) -> Result<Option<Head>> {
        self.provider.outcome(r)
    }
    fn decide(&mut self, old: Head, new: Head) -> Result<()> {
        self.provider.decide(old, new)?;
        if self.lose {
            self.lose = false;
            Err(Fault::Uncertain)
        } else {
            Ok(())
        }
    }
}

#[test]
fn admission_genesis_and_unknown_outcome() {
    for sector in [512, 4096] {
        let mut ledger = Ledger::new(SimDevice::new(12, sector).unwrap());
        let mut dirty = SimDevice::new(512, sector).unwrap();
        dirty.write(511, &[7; 4096]).unwrap();
        dirty.flush().unwrap();
        let ctrl = dirty.control();
        let before = ctrl.snapshot().bytes().unwrap();
        assert!(matches!(
            provision(dirty, IDS, DOMAIN, [5; 16], &mut ledger),
            Err(Fault::NotNew)
        ));
        assert_eq!(ctrl.snapshot().bytes().unwrap(), before);
        let mut fs = provision(
            SimDevice::new(512, sector).unwrap(),
            IDS,
            DOMAIN,
            [5; 16],
            &mut ledger,
        )
        .unwrap();
        let proof = Enrollment::from_provisioner(&mut ledger).unwrap();
        let mut auth = LoseAck {
            provider: ledger,
            lose: true,
        };
        let mut reg = Registry::open(&mut fs, &mut auth, DOMAIN).unwrap();
        assert!(matches!(
            reg.enroll(&proof, [6; 16], "owner", [7; 16], "grp"),
            Err(Fault::Uncertain)
        ));
        let disk = fs.unmount().unwrap();
        let mut fs = Engine::mount_current(disk, IDS, Mode::ReadWrite).unwrap();
        let mut reg = Registry::open(&mut fs, &mut auth, DOMAIN).unwrap();
        let first = reg.outcome([6; 16]).unwrap().unwrap();
        assert_eq!(first.generation, 1);
        assert!(matches!(
            reg.enroll(&proof, [6; 16], "owner", [7; 16], "grp"),
            Err(Fault::Denied)
        ));
        let admin = reg.resolve_verified_factor([7; 16]).unwrap();
        let other = reg
            .mutate(admin, [8; 16], Change::AddGroup("extra"))
            .unwrap();
        let grant = reg.grant_attested(admin.subject()).unwrap();
        reg.mutate(
            grant,
            [9; 16],
            Change::AddMember(admin.subject(), created_group(&other)),
        )
        .unwrap();
        let grant = reg.grant_attested(admin.subject()).unwrap();
        assert_eq!(reg.groups(grant).unwrap().len(), 2);
        assert!(matches!(
            reg.mutate(grant, [10; 16], Change::Delete(admin.subject())),
            Err(Fault::Denied)
        ));
        let mut p8 = Engine::format(SimDevice::new(512, sector).unwrap(), IDS).unwrap();
        let receipt = p8.receipt();
        assert!(matches!(
            Registry::open(&mut p8, &mut auth, DOMAIN),
            Err(Fault::WrongDomain)
        ));
        assert_eq!(p8.receipt(), receipt);
    }
}

/// Témoins de rattachement, distincts des services de cache, VFS et planification.
#[test]
fn objets_du_titulaire_ne_suivent_pas_un_nom_reutilise() {
    let mut a = Ledger::new(SimDevice::new(12, 4096).unwrap());
    let mut fs = provision(
        SimDevice::new(512, 4096).unwrap(),
        IDS,
        DOMAIN,
        [8; 16],
        &mut a,
    )
    .unwrap();
    let proof = Enrollment::from_provisioner(&mut a).unwrap();
    let mut reg = Registry::open(&mut fs, &mut a, DOMAIN).unwrap();
    let root = reg
        .enroll(&proof, [1; 16], "admin", [1; 16], "principal")
        .unwrap();
    let admin = created_user(&root);
    let group = created_group(&root);
    let c = |reference| Change::Create {
        name: "alice",
        credential: reference,
        group,
    };
    let old = reg
        .mutate(reg.grant_attested(admin).unwrap(), [2; 16], c([2; 16]))
        .unwrap();
    let old_id = created_user(&old);
    let handle = reg.grant_attested(old_id).unwrap();
    let objets = [("cache", old_id), ("fichier", old_id), ("crontab", old_id)];
    reg.mutate(
        reg.grant_attested(admin).unwrap(),
        [3; 16],
        Change::Delete(old_id),
    )
    .unwrap();
    reg.mutate(reg.grant_attested(admin).unwrap(), [4; 16], c([4; 16]))
        .unwrap();
    let fresh = reg.resolve_verified_factor([4; 16]).unwrap();
    for (_, owner) in objets {
        assert!(matches!(reg.inspect(fresh, owner), Err(Fault::Denied)));
        assert!(matches!(reg.inspect(handle, owner), Err(Fault::Denied)));
    }
}
