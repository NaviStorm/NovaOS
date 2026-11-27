mod support;
use State::*;
use support::*;
fn reach(m: &mut Manager<'_, '_>, wanted: State) -> Stamp {
    let d = m.discover(sl(0)).unwrap();
    if wanted == Discovered {
        return d;
    }
    if wanted == Removed {
        return m.step(d, Event::Remove).unwrap();
    }
    let v = checked(m, d).unwrap();
    if wanted == Verified {
        return v;
    }
    let r = m.reserve(v, 16, 1, 1).unwrap();
    if wanted == Reserved {
        return r;
    }
    if matches!(wanted, Returned | Readmitting) {
        let end = m.step(r, Event::Cancel).unwrap();
        return if wanted == Returned {
            end
        } else {
            m.step(end, Event::Readmit).unwrap()
        };
    }
    if matches!(wanted, Reconciling | Quarantined) {
        let rec = m.step(r, Event::Uncertain).unwrap();
        return if wanted == Reconciling {
            rec
        } else {
            m.step(rec, Event::Quarantine).unwrap()
        };
    }
    let (s, ticket) = m.prepare_launch(r).unwrap();
    let r = m.launch(s, ticket).unwrap();
    let a = m.step(r, Event::Assign).unwrap();
    if wanted == Assigned {
        return a;
    }
    let l = m.group(sl(0)).unwrap().lease().unwrap();
    m.readiness(l, true, true, true).unwrap();
    let i = m.step(a, Event::Initialize).unwrap();
    if wanted == Initializing {
        return i;
    }
    let active = m.step(i, Event::Publish).unwrap();
    if wanted == Active {
        return active;
    }
    if wanted == Suspended {
        return m.step(active, Event::Suspend).unwrap();
    }
    let rev = m.driver_death(l).unwrap();
    if wanted == Revoking {
        return rev;
    }
    m.step(rev, Event::Stop).unwrap()
}
fn attempt(
    m: &mut Manager<'_, '_>,
    s: Stamp,
    from: State,
    to: State,
) -> novaos_gestionnaire_sim::Result<Stamp> {
    let l = m.group(sl(0))?.lease();
    match to {
        Discovered => m.step(
            s,
            if from == Readmitting {
                Event::Rediscover
            } else {
                Event::Invalidate
            },
        ),
        Verified => checked(m, s),
        Reserved => m.reserve(s, 17, 2, 2),
        Assigned if from == Reserved => {
            let (s, t) = m.prepare_launch(s)?;
            let s = m.launch(s, t)?;
            m.step(s, Event::Assign)
        }
        Assigned => m.step(s, Event::Assign),
        Initializing => {
            if from == Assigned {
                m.readiness(l.unwrap(), true, true, true)?;
            }
            if from == Suspended {
                m.readiness(l.unwrap(), true, true, true)?;
                m.step(s, Event::Resume)
            } else {
                m.step(s, Event::Initialize)
            }
        }
        Active => m.step(s, Event::Publish),
        Suspended => m.step(s, Event::Suspend),
        Revoking => m.step(s, Event::Revoke),
        Stopping => m.step(s, Event::Stop),
        Removed => m.step(s, Event::Remove),
        Quarantined => m.step(s, Event::Quarantine),
        Readmitting => m.step(s, Event::Readmit),
        Reconciling => {
            if from == Quarantined {
                m.evidence(proof(l.unwrap(), 1, 1))?;
            }
            m.step(
                s,
                if from == Reserved {
                    Event::Uncertain
                } else {
                    Event::Reconcile
                },
            )
        }
        Returned => {
            if from == Reconciling {
                m.evidence(proof(l.unwrap(), 1, PAGES))?;
            }
            m.step(
                s,
                if from == Reserved {
                    Event::Cancel
                } else {
                    Event::Return
                },
            )
        }
    }
}
#[test]
fn toutes_les_transitions_et_tous_les_refus() {
    let names = [
        "Decouvert",
        "Verifie",
        "Reserve",
        "Attribue",
        "Initialisation",
        "Actif",
        "Suspendu",
        "Revocation",
        "Arret",
        "RetraitMateriel",
        "Reconciliation",
        "Quarantaine",
        "Restitue",
        "Readmission",
    ];
    let spec = include_str!("../../../docs/gestionnaire-pilotes/cycle-et-ipc.md")
        .split("stateDiagram-v2")
        .nth(1)
        .unwrap()
        .split("```")
        .next()
        .unwrap();
    let edges: Vec<_> = spec
        .lines()
        .filter_map(|l| l.trim().split_once(" --> "))
        .collect();
    assert_eq!(edges.len(), 32);
    let p = lab::policy();
    let c = catalog(&p, 1);
    let mut allowed = 1;
    for from in STATES {
        for to in STATES {
            let expect = edges.contains(&(names[from as usize], names[to as usize]));
            assert_eq!(permits(Some(from), to), expect);
            let mut m = sim(&c, 1);
            let s = reach(&mut m, from);
            let u = m.usage().unwrap();
            let result = attempt(&mut m, s, from, to);
            assert_eq!(result.is_ok(), expect, "{from:?} → {to:?} {result:?}");
            if expect {
                allowed += 1;
                assert_eq!(m.group(sl(0)).unwrap().state(), to)
            } else {
                assert_eq!(m.group(sl(0)).unwrap().stamp(), s);
                assert_eq!(m.usage().unwrap(), u)
            }
        }
    }
    assert_eq!(allowed, 32);
}
#[test]
fn recus_partiels_generations_et_quarantaine() {
    let p = lab::policy();
    let c = catalog(&p, 1);
    let mut m = sim(&c, 2);
    let a = reserve(&mut m, 0, 1);
    let a = activate(&mut m, a);
    let independent = reserve(&mut m, 1, 2);
    let independent = activate(&mut m, independent);
    let l = m.group(sl(0)).unwrap().lease().unwrap();
    let leaf = m.leaf(l, Right::DmaWrite, "rx").unwrap();
    assert!(
        m.authorize(leaf, l.holder, l.incarnation, Right::DmaWrite, "rx")
            .is_ok()
    );
    for right in [Right::DmaRead, Right::Irq] {
        assert!(
            m.authorize(leaf, l.holder, l.incarnation, right, "rx")
                .is_err()
        )
    }
    assert!(
        m.authorize(leaf, l.holder + 1, l.incarnation, Right::DmaWrite, "rx")
            .is_err()
    );
    let suspend = m.step(a, Event::Suspend).unwrap();
    m.readiness(l, true, true, false).unwrap();
    let init = m.step(suspend, Event::Resume).unwrap();
    m.step(init, Event::Publish).unwrap();
    assert!(
        m.authorize(leaf, l.holder, l.incarnation, Right::DmaWrite, "rx")
            .is_err()
    );
    let dead = m.driver_death(l).unwrap();
    let stop = m.step(dead, Event::Stop).unwrap();
    let rec = m.step(stop, Event::Reconcile).unwrap();
    let q = m.step(rec, Event::Quarantine).unwrap();
    assert_eq!(m.step(q, Event::Reconcile), Err(Error::Uncertain));
    let foreign = m.group(sl(1)).unwrap().lease().unwrap();
    assert!(m.evidence(proof(foreign, 1, 1)).is_err());
    assert!(
        m.evidence(proof(Lease { generation: 2, ..l }, 1, 1))
            .is_err()
    );
    m.evidence(proof(l, 1, 1)).unwrap();
    assert_eq!(m.group(sl(0)).unwrap().held().dma, 21);
    assert!(m.evidence(proof(l, 1, 1)).is_err());
    let rec = m.step(q, Event::Reconcile).unwrap();
    assert_eq!(m.step(rec, Event::Return), Err(Error::Uncertain));
    m.evidence(proof(l, 2, PAGES ^ 1)).unwrap();
    m.step(rec, Event::Return).unwrap();
    assert_eq!(m.group(sl(1)).unwrap().stamp(), independent);
    assert!(m.leaf(foreign, Right::Class, "rx").is_ok());
}
#[test]
fn gardes_reservation_et_q35() {
    let p = lab::policy();
    let c = catalog(&p, 1);
    let mut m = Manager::new(4, &c, 1, Use::LAB, Use::default(), Some(1), u64::MAX - 1).unwrap();
    m.refresh(1, true, [node(0)]).unwrap();
    let d = m.discover(sl(0)).unwrap();
    let v = checked(&mut m, d).unwrap();
    let r = m.reserve(v, 16, 1, 1).unwrap();
    assert_eq!(r.generation, u64::MAX);
    assert_eq!(m.reserve(v, 16, 2, 1), Err(Error::Stale));
    m.nvme_lease(Some(12)).unwrap();
    let end = m.step(r, Event::Cancel).unwrap();
    assert_eq!(m.step(end, Event::Readmit), Err(Error::Denied));
    assert_eq!(m.usage().unwrap(), Use::default());
    m.nvme_lease(None).unwrap();
    let again = m.step(end, Event::Readmit).unwrap();
    let d = m.step(again, Event::Rediscover).unwrap();
    let v = checked(&mut m, d).unwrap();
    assert_eq!(m.reserve(v, 16, 2, 1), Err(Error::Exhausted));
    assert_eq!(m.usage().unwrap(), Use::default());
}
#[test]
fn plans_bornes_revisions_et_quotas() {
    let p = lab::policy();
    let c = catalog(&p, 64);
    let mut m = sim(&c, 3);
    let a = m.discover(sl(0)).unwrap();
    let b = m.discover(sl(1)).unwrap();
    let d = m.discover(sl(2)).unwrap();
    let a = checked(&mut m, a).unwrap();
    let b = checked(&mut m, b).unwrap();
    assert_eq!(m.plan(sl(0)).unwrap().version(), [1, 63, 0]);
    assert_eq!(checked(&mut m, d), Err(Error::Full));
    m.step(b, Event::Invalidate).unwrap();
    checked(&mut m, d).unwrap();
    m.refresh(2, true, (0..3).map(node)).unwrap();
    assert_eq!(m.reserve(a, 16, 1, 1), Err(Error::Stale));
    let s = m.step(a, Event::Invalidate).unwrap();
    let v = checked(&mut m, s).unwrap();
    m.change_catalog(&c, 2, 2).unwrap();
    assert_eq!(m.reserve(v, 16, 1, 1), Err(Error::Stale));
    let low = Use {
        pages: 32,
        ..Use::LAB
    };
    let mut m = Manager::new(1, &c, 1, low, Use::default(), None, 0).unwrap();
    m.refresh(1, true, [node(0)]).unwrap();
    let d = m.discover(sl(0)).unwrap();
    let v = checked(&mut m, d).unwrap();
    assert_eq!(m.reserve(v, 16, 1, 1), Err(Error::Full));
    assert_eq!(m.usage().unwrap(), Use::default());
    assert!(m.plan(sl(0)).is_some());
}
