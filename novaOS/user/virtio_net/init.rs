//! Séquence VirtIO moderne indépendante du transport privilégié.
//! Ce trait interne ne publie aucun appel système et ne véhicule aucune adresse.
use super::{Direction, Queue, SLOTS, features};

pub const ACK: u8 = 1;
pub const DRIVER: u8 = 2;
pub const DRIVER_OK: u8 = 4;
pub const FEATURES_OK: u8 = 8;
pub const NEEDS_RESET: u8 = 64;
pub const FAILED: u8 = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitError {
    Transport,
    Status,
    Features,
    Geometry,
    Mac,
    Quarantined,
}

type Result<T> = core::result::Result<T, InitError>;

pub trait Control {
    /// Toutes les opérations sont bornées. Le reset attend l'observation de zéro.
    fn reset(&mut self) -> Result<()>;
    fn status(&mut self) -> Result<u8>;
    fn set_status(&mut self, status: u8) -> Result<()>;
    fn offered(&mut self) -> Result<u64>;
    fn accept(&mut self, features: u64) -> Result<()>;
    /// Lecture cohérente de la MAC encadrée par config_generation.
    fn mac(&mut self) -> Result<[u8; 6]>;
    fn max_queue(&mut self, direction: Direction) -> Result<u16>;
    fn setup(&mut self, direction: Direction, size: u16) -> Result<()>;
    fn post_rx(&mut self, slot: u16) -> Result<()>;
    fn kick_rx(&mut self) -> Result<()>;
    /// Confirmer arrêt, invalidation et restitution, sinon conserver le bail.
    fn retire(&mut self) -> Result<()>;
}

pub struct Link {
    pub mac: [u8; 6],
    pub rx: Queue,
    pub tx: Queue,
}

pub fn initialise(bus: &mut impl Control) -> Result<Link> {
    match start(bus) {
        Ok(link) => Ok(link),
        Err(reason) => match bus.retire() {
            Ok(()) => Err(reason),
            Err(_) => Err(InitError::Quarantined),
        },
    }
}

fn transition(bus: &mut impl Control, expected: u8) -> Result<()> {
    bus.set_status(expected)?;
    let observed = bus.status()?;
    if observed & expected != expected || observed & (NEEDS_RESET | FAILED) != 0 {
        return Err(InitError::Status);
    }
    Ok(())
}

fn start(bus: &mut impl Control) -> Result<Link> {
    bus.reset()?;
    if bus.status()? != 0 {
        return Err(InitError::Status);
    }
    transition(bus, ACK)?;
    transition(bus, ACK | DRIVER)?;
    let selected = features(bus.offered()?).map_err(|_| InitError::Features)?;
    bus.accept(selected)?;
    transition(bus, ACK | DRIVER | FEATURES_OK)?;
    let mac = bus.mac()?;
    if mac[0] & 1 != 0 || mac == [0; 6] {
        return Err(InitError::Mac);
    }
    for direction in [Direction::Receive, Direction::Transmit] {
        if bus.max_queue(direction)? < SLOTS {
            return Err(InitError::Geometry);
        }
        bus.setup(direction, SLOTS)?;
    }
    let mut rx = Queue::new(Direction::Receive);
    for _ in 0..SLOTS {
        bus.post_rx(rx.reserve().map_err(|_| InitError::Geometry)?)?;
    }
    transition(bus, ACK | DRIVER | FEATURES_OK | DRIVER_OK)?;
    bus.kick_rx()?;
    Ok(Link {
        mac,
        rx,
        tx: Queue::new(Direction::Transmit),
    })
}
