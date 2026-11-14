use super::*;

const GROUP: Group = Group { unit: 2, group: 7 };
const PID: u64 = 9;
const PAGE: Page = Page {
    frame: 0x1000,
    iova: 0x8000,
    direction: Direction::ReadByDevice,
};

#[derive(Default)]
struct Model {
    events: Vec<&'static str>,
    blocked: Option<&'static str>,
    pins: usize,
    translated: bool,
    executing: bool,
    denied_source: bool,
}
impl Model {
    fn event(&mut self, tag: &'static str) -> Result<()> {
        self.events.push(tag);
        if self.blocked == Some(tag) {
            Err(Error::Busy)
        } else {
            Ok(())
        }
    }
}
impl Unit for Model {
    fn permits_group(&self, group: Group, pid: u64) -> bool {
        group == GROUP && pid == PID && !self.denied_source
    }
    fn attach_closed(&mut self, _: Token) -> Result<()> {
        assert!(!self.executing && !self.translated);
        self.event("attach")
    }
    fn pin(&mut self, token: Token, page: Page) -> Result<()> {
        if (token.owner, page.frame, page.direction) != (PID, PAGE.frame, PAGE.direction) {
            return Err(Error::Denied);
        }
        self.event("pin")?;
        self.pins += 1;
        Ok(())
    }
    fn configure_page(&mut self, _: Token, _: Page) -> Result<()> {
        assert!(self.pins > 0);
        self.translated = true;
        self.event("map")
    }
    fn publish_and_open(&mut self, _: Token) -> Result<()> {
        assert!(self.translated && self.pins > 0);
        self.event("sync")?;
        self.executing = true;
        self.event("open")
    }
    fn retire_and_confirm(&mut self, _: Token) -> Result<()> {
        self.event("drain")?;
        self.executing = false;
        self.event("invalidate")?;
        self.event("confirm")?;
        self.translated = false;
        Ok(())
    }
    fn unpin(&mut self, _: Token, _: Page) {
        assert!(!self.executing && !self.translated);
        self.events.push("unpin");
        self.pins -= 1;
    }
}

#[test]
fn dma_physique_ferme_et_aucun_effet_sans_isolation() {
    assert_eq!(physical_domain(GROUP, PID), Err(Error::Denied));
    let mut d = Domain::<_, 1>::new(
        GROUP,
        Model {
            denied_source: true,
            ..Default::default()
        },
    );
    assert_eq!(d.attach(PID), Err(Error::Denied));
    assert!(d.unit.events.is_empty());
}

#[test]
fn droits_sens_quotas_et_proprietaire() {
    let mut d = Domain::<_, 1>::new(GROUP, Model::default());
    let t = d.attach(PID).unwrap();
    assert_eq!(d.publish(PID, t), Err(Error::Denied));
    assert_eq!(d.map(PID + 1, t, PAGE), Err(Error::Denied));
    for bad in [Page { frame: 1, ..PAGE }, Page { iova: 0, ..PAGE }] {
        assert_eq!(d.map(PID, t, bad), Err(Error::Geometry));
    }
    for bad in [
        Page {
            frame: 0xa000,
            ..PAGE
        },
        Page {
            direction: Direction::WrittenByDevice,
            ..PAGE
        },
    ] {
        assert_eq!(d.map(PID, t, bad), Err(Error::Denied));
    }
    assert_eq!(d.unit.events, ["attach"]);
    d.map(PID, t, PAGE).unwrap();
    assert_eq!(d.map(PID, t, PAGE), Err(Error::Busy));
    assert_eq!(
        d.map(
            PID,
            t,
            Page {
                frame: 0x9000,
                iova: 0xa000,
                ..PAGE
            }
        ),
        Err(Error::Exhausted)
    );
    d.publish(PID, t).unwrap();
    assert_eq!(d.map(PID, t, PAGE), Err(Error::Denied));
    assert!(d.unit.executing);
    d.revoke(PID, t).unwrap();
    assert_eq!(
        d.unit.events,
        [
            "attach",
            "pin",
            "map",
            "sync",
            "open",
            "drain",
            "invalidate",
            "confirm",
            "unpin"
        ]
    );
    assert_eq!(d.unit.pins, 0);
}

#[test]
fn pages_retenues_apres_retrait_non_confirme_et_capacite_perimee() {
    for blocked in ["drain", "invalidate", "confirm"] {
        let mut d = Domain::<_, 1>::new(GROUP, Model::default());
        let t = d.attach(PID).unwrap();
        d.map(PID, t, PAGE).unwrap();
        d.publish(PID, t).unwrap();
        d.unit.blocked = Some(blocked);
        assert_eq!(d.revoke(PID, t), Err(Error::Quarantined));
        assert_eq!(d.attach(PID), Err(Error::Busy));
        assert_eq!(d.map(PID, t, PAGE), Err(Error::Denied));
        assert_eq!(d.unit.pins, 1);
        assert!(!d.unit.events.contains(&"unpin"));
        d.unit.blocked = None;
        d.retire().unwrap();
        let new = d.attach(PID).unwrap();
        assert_ne!(t, new);
        assert_eq!(d.revoke(PID, t), Err(Error::Denied));
        assert_eq!(d.unit.pins, 0);
    }
}

#[test]
fn mapping_et_publication_partiels_conservent_les_cadres() {
    for blocked in ["attach", "map", "sync", "open"] {
        let mut d = Domain::<_, 1>::new(
            GROUP,
            Model {
                blocked: Some(blocked),
                ..Default::default()
            },
        );
        let result = d
            .attach(PID)
            .and_then(|t| d.map(PID, t, PAGE).and_then(|_| d.publish(PID, t)));
        assert_eq!(result, Err(Error::Quarantined));
        assert_eq!(d.unit.pins, usize::from(blocked != "attach"));
        assert_eq!(d.attach(PID), Err(Error::Busy));
        d.unit.blocked = None;
        d.retire().unwrap();
        assert_eq!(d.unit.pins, 0);
    }
}
