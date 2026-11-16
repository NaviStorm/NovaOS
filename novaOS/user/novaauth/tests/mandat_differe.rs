//! Témoin de réévaluation. Le pilote conserve un mandat attesté par le banc.
//! Aucun client ou processus ordinaire ne reçoit grant_attested.
use novafs::{Ids, device::SimDevice};
use novaos_identity::{lab::Ledger, *};

#[test]
fn reevaluation_du_mandat_apres_deconnexion_retrait_et_suppression() {
    for sector in [512, 4096] {
        let domain = [41; 16];
        let mut authority = Ledger::new(SimDevice::new(12, sector).unwrap());
        let mut fs = provision(
            SimDevice::new(512, sector).unwrap(),
            Ids {
                fs: [31; 16],
                lv: [32; 16],
            },
            domain,
            [5; 16],
            &mut authority,
        )
        .unwrap();
        let permit = Enrollment::from_provisioner(&mut authority).unwrap();
        let mut reg = Registry::open(&mut fs, &mut authority, domain).unwrap();
        let first = reg
            .enroll(&permit, [1; 16], "admin", [9; 16], "travail")
            .unwrap();
        let admin = created_user(&first);
        let group = created_group(&first);
        let create = |factor| Change::Create {
            name: "alice",
            credential: factor,
            group,
        };
        let head = reg
            .mutate(reg.grant_attested(admin).unwrap(), [2; 16], create([6; 16]))
            .unwrap();
        let owner = created_user(&head);
        // Le mandat ne contient que le titulaire stable et le groupe requis.
        // La session, son facteur et ses droits temporaires sont hors du contrat.
        let mandate = (owner, group);
        let evaluate = |reg: &Registry<'_, SimDevice, Ledger<SimDevice>>| -> Result<Id> {
            let current = reg.grant_attested(mandate.0)?;
            if !reg.groups(current)?.contains(&mandate.1) {
                return Err(Fault::Denied);
            }
            Ok(current.subject())
        };
        assert_eq!(evaluate(&reg).unwrap(), owner);
        reg.mutate(
            reg.grant_attested(admin).unwrap(),
            [3; 16],
            Change::RemoveMember(owner, group),
        )
        .unwrap();
        assert!(matches!(evaluate(&reg), Err(Fault::Denied)));
        reg.mutate(
            reg.grant_attested(admin).unwrap(),
            [4; 16],
            Change::Delete(owner),
        )
        .unwrap();
        assert!(matches!(evaluate(&reg), Err(Fault::Denied)));
        let new = reg
            .mutate(reg.grant_attested(admin).unwrap(), [5; 16], create([7; 16]))
            .unwrap();
        assert_ne!(created_user(&new), owner);
        assert!(matches!(evaluate(&reg), Err(Fault::Denied)));
        assert!(reg.grant_attested(admin).is_ok());
    }
}
