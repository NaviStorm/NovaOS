use crate::inventory::Attachment;
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[repr(u8)]
pub enum State {
    Discovered,
    Verified,
    Reserved,
    Assigned,
    Initializing,
    Active,
    Suspended,
    Revoking,
    Stopping,
    Removed,
    Reconciling,
    Quarantined,
    Returned,
    Readmitting,
}
use State::*;
pub const STATES: [State; 14] = [
    Discovered,
    Verified,
    Reserved,
    Assigned,
    Initializing,
    Active,
    Suspended,
    Revoking,
    Stopping,
    Removed,
    Reconciling,
    Quarantined,
    Returned,
    Readmitting,
];
/// L'entrée sans état compte parmi les 32 transitions de 4A.
pub const EDGES: [(Option<State>, State); 32] = [
    (None, Discovered),
    (Some(Discovered), Verified),
    (Some(Verified), Discovered),
    (Some(Verified), Reserved),
    (Some(Reserved), Assigned),
    (Some(Reserved), Reconciling),
    (Some(Reserved), Returned),
    (Some(Assigned), Initializing),
    (Some(Initializing), Active),
    (Some(Active), Suspended),
    (Some(Suspended), Initializing),
    (Some(Assigned), Revoking),
    (Some(Initializing), Revoking),
    (Some(Active), Revoking),
    (Some(Suspended), Revoking),
    (Some(Revoking), Stopping),
    (Some(Stopping), Reconciling),
    (Some(Discovered), Removed),
    (Some(Verified), Removed),
    (Some(Reserved), Removed),
    (Some(Assigned), Removed),
    (Some(Initializing), Removed),
    (Some(Active), Removed),
    (Some(Suspended), Removed),
    (Some(Revoking), Removed),
    (Some(Stopping), Removed),
    (Some(Removed), Reconciling),
    (Some(Reconciling), Quarantined),
    (Some(Reconciling), Returned),
    (Some(Quarantined), Reconciling),
    (Some(Returned), Readmitting),
    (Some(Readmitting), Discovered),
];
pub fn permits(from: Option<State>, to: State) -> bool {
    EDGES.contains(&(from, to))
}
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Lease {
    pub attachment: Attachment,
    pub inventory: u64,
    pub holder: u64,
    pub incarnation: u64,
    pub generation: u64,
    pub domain: u32,
    pub profile: [u8; 32],
}
/// Entrée de l'oracle synthétique du banc, indépendante du canal pilote.
#[derive(Debug, Copy, Clone)]
pub struct Observation {
    pub lease: Lease,
    pub serial: u64,
    pub source_stopped: bool,
    pub stopped_pages: crate::dma::PageSet,
    pub translations_removed: crate::dma::PageSet,
    pub ownership_returned: crate::dma::PageSet,
}
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub struct Stamp {
    pub attachment: Attachment,
    pub revision: u64,
    pub generation: u64,
}
#[derive(Clone, Copy, Debug)]
pub enum Event {
    Invalidate,
    Assign,
    Initialize,
    Publish,
    Suspend,
    Resume,
    Revoke,
    Stop,
    Uncertain,
    Cancel,
    Remove,
    Reconcile,
    Quarantine,
    Return,
    Readmit,
    Rediscover,
}
impl Event {
    pub fn destination(self) -> State {
        match self {
            Self::Invalidate | Self::Rediscover => Discovered,
            Self::Assign => Assigned,
            Self::Initialize | Self::Resume => Initializing,
            Self::Publish => Active,
            Self::Suspend => Suspended,
            Self::Revoke => Revoking,
            Self::Stop => Stopping,
            Self::Uncertain | Self::Reconcile => Reconciling,
            Self::Cancel | Self::Return => Returned,
            Self::Remove => Removed,
            Self::Quarantine => Quarantined,
            Self::Readmit => Readmitting,
        }
    }
}
