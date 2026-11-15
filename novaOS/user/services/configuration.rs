//! État attesté sur le disque. Les déclarations actives appartiennent à Policy jusqu'au reboot.
use super::{
    config_client::{BUSY, CORRUPT, Client, UNCERTAIN},
    control,
    policy::Policy,
    rt,
    settings::{self, Records},
};

pub struct Configuration {
    client: Option<Client>,
    pub records: Records,
    pub stored: u64,
    pub applied: u64,
    pub status: u64,
}
impl Configuration {
    pub fn open(tx: u64, rx: u64) -> Self {
        let mut c = Self {
            client: if tx != 0 && rx != 0 {
                Some(Client::new(tx, rx))
            } else {
                None
            },
            records: settings::defaults(),
            stored: 0,
            applied: 0,
            status: rt::CLOSED,
        };
        c.refresh();
        c.applied = c.stored;
        c
    }

    fn decode(data: &[u8], generation_lue: u64, n: usize) -> rt::Result<Records> {
        match (generation_lue, n) {
            (0, 0) => Ok(settings::defaults()),
            (0, _) | (_, 0) => Err(CORRUPT),
            _ => {
                let records = settings::decode(data.get(..n).ok_or(CORRUPT)?)
                    .ok_or(CORRUPT)?;
                Policy::configured(&records).map_err(|_| CORRUPT)?;
                Ok(records)
            }
        }
    }

    pub fn refresh(&mut self) {
        let Some(client) = self.client.as_mut() else {
            self.recovery_catalogue(rt::CLOSED);
            return;
        };
        let mut data = [0; settings::BYTES];
        let current = client.load(&mut data).and_then(|(generation_lue, n)| {
            Self::decode(&data, generation_lue, n).map(|r| (generation_lue, r))
        });
        match current {
            Ok((generation_lue, r)) => {
                self.stored = generation_lue;
                self.records = r;
                self.status = rt::OK;
            }
            Err(e) => {
                // Seule une corruption vérifiée autorise la lecture de l'autre
                // génération. Une panne d'E/S peut cacher une version plus récente intacte.
                if e == CORRUPT
                    && let Ok((generation_lue, n)) = client.previous(&mut data)
                    && generation_lue != 0
                    && let Ok(r) = Self::decode(&data, generation_lue, n)
                {
                    self.status = e;
                    self.records = r;
                    self.stored = generation_lue;
                    return;
                }
                self.recovery_catalogue(e);
            }
        }
    }

    fn recovery_catalogue(&mut self, error: u64) {
        self.status = error;
        self.records = settings::defaults();
        for record in &mut self.records {
            record.auto = false;
        }
        self.stored = 0;
    }

    pub fn autostart(&mut self, id: u64, auto: bool, generation: u64) -> rt::Result<()> {
        if self.status != rt::OK {
            return Err(self.status);
        }
        if self.stored != generation {
            return Err(BUSY);
        }
        let mut next = self.records;
        let d = next.iter_mut().find(|d| d.id == id).ok_or(rt::REFUSED)?;
        d.auto = auto;
        Policy::configured(&next).map_err(control::error)?;
        let mut bytes = [0; settings::BYTES];
        let len = settings::encode(&next, &mut bytes).ok_or(rt::LIMIT)?;
        let client = self.client.as_mut().ok_or(rt::DENIED)?;
        match client.save(generation, &bytes[..len]) {
            Ok(generation_lue) if Some(generation_lue) == generation.checked_add(1) => {
                self.records = next;
                self.stored = generation_lue;
                Ok(())
            }
            Ok(_) | Err(UNCERTAIN) => {
                self.status = UNCERTAIN;
                Err(UNCERTAIN)
            }
            Err(e @ (BUSY | rt::REVOKED)) => {
                self.status = e;
                Err(e)
            }
            Err(e) => Err(e),
        }
    }
}
