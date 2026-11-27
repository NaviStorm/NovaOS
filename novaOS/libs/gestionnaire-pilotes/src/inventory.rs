use crate::{Error, Result, catalogue::Bus, next};
use alloc::{string::String, vec::Vec};

pub const MAX_FUNCTIONS: usize = 128;
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Slot(u8);
impl Slot {
    pub fn new(n: u8) -> Result<Self> {
        if n < 128 {
            Ok(Self(n))
        } else {
            Err(Error::Format)
        }
    }
    pub fn number(self) -> u8 {
        self.0
    }
    pub fn bit(self) -> u128 {
        1u128 << self.0
    }
    pub fn label(self) -> [u8; 7] {
        let n = self.0;
        [
            b'd',
            b'e',
            b'v',
            b'-',
            b'0' + n / 100,
            b'0' + (n / 10) % 10,
            b'0' + n % 10,
        ]
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Address {
    Pci {
        segment: u16,
        bus: u8,
        device: u8,
        function: u8,
        express: bool,
        bridge: bool,
    },
    Usb {
        port: u8,
        configuration: u8,
        interface: Option<u8>,
    },
    Acpi(u32),
    DeviceTree(u32),
    Virtual {
        provider: u32,
        instance: u32,
    },
}
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Ownership {
    Independent,
    Parent,
    Unknown,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Node {
    pub slot: Slot,
    pub appearance: u64,
    pub controller: u16,
    pub address: Address,
    pub identity: Bus,
    pub parent: Option<Slot>,
    pub needs: u128,
    pub aliases: u128,
    pub reset: u16,
    pub irq_scope: u16,
    pub ownership: Ownership,
    pub provider_trusted: bool,
}
fn name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 96
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
fn compact(s: &mut String) -> Result<()> {
    if !name(s) {
        return Err(Error::Format);
    }
    if s.capacity() != s.len() {
        let mut copy = String::new();
        copy.try_reserve_exact(s.len()).map_err(|_| Error::Memory)?;
        copy.push_str(s);
        *s = copy;
    }
    Ok(())
}
impl Node {
    fn validate(&mut self) -> Result<()> {
        if self.appearance == 0
            || self.controller == 0
            || self.parent == Some(self.slot)
            || self.needs & self.slot.bit() != 0
        {
            return Err(Error::Format);
        }
        if matches!(
            self.address,
            Address::Usb {
                interface: Some(_),
                ..
            }
        ) && (self.ownership != Ownership::Parent || self.parent.is_none())
        {
            return Err(Error::Denied);
        }
        match (self.address, &mut self.identity) {
            (
                Address::Pci {
                    device, function, ..
                },
                Bus::Pci { .. },
            ) if device < 32 && function < 8 => {}
            (
                Address::Usb {
                    port,
                    configuration,
                    interface,
                },
                Bus::Usb {
                    parent,
                    configuration: c,
                    interface: i,
                    ..
                },
            ) if port != 0 && Some(configuration) == *c && interface == *i => {
                if let Some(s) = parent {
                    compact(s)?;
                    if !self
                        .parent
                        .is_some_and(|p| p.label().as_slice() == s.as_bytes())
                    {
                        return Err(Error::Format);
                    }
                }
            }
            (
                Address::Acpi(_) | Address::DeviceTree(_) | Address::Virtual { .. },
                Bus::Platform {
                    provider,
                    firmware_id,
                },
            ) => {
                compact(provider)?;
                compact(firmware_id)?;
                if matches!(self.address, Address::Acpi(_)) && provider != "acpi"
                    || matches!(self.address, Address::DeviceTree(_)) && provider != "dt"
                {
                    return Err(Error::Format);
                }
            }
            (_, Bus::Unknown) => {}
            _ => return Err(Error::Format),
        }
        Ok(())
    }
    pub(crate) fn requires(&self) -> u128 {
        self.needs | self.parent.map_or(0, Slot::bit)
    }
    fn usb_config_conflict(&self, other: &Self) -> bool {
        if self.controller != other.controller || self.parent != other.parent {
            return false;
        }
        matches!((self.address,other.address),(Address::Usb{port:a,configuration:x,..},Address::Usb{port:b,configuration:y,..}) if a==b && x!=y)
    }
    fn same_place(&self, other: &Self) -> bool {
        if self.controller != other.controller {
            return false;
        }
        match (self.address, other.address) {
            (
                Address::Pci {
                    segment: a,
                    bus: b,
                    device: c,
                    function: d,
                    ..
                },
                Address::Pci {
                    segment: w,
                    bus: x,
                    device: y,
                    function: z,
                    ..
                },
            ) => (a, b, c, d) == (w, x, y, z),
            (Address::Usb { .. }, Address::Usb { .. }) => {
                self.parent == other.parent && self.address == other.address
            }
            _ => self.address == other.address,
        }
    }
    pub fn external_bytes(&self) -> usize {
        match &self.identity {
            Bus::Platform {
                provider,
                firmware_id,
            } => provider.capacity() + firmware_id.capacity(),
            Bus::Usb {
                parent: Some(s), ..
            } => s.capacity(),
            _ => 0,
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Attachment {
    pub boot: u64,
    pub controller: u16,
    pub slot: Slot,
    pub appearance: u64,
    pub topology: u64,
}
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Scope {
    pub members: u128,
    pub reset_peers: u128,
    pub dependencies: u128,
}
pub struct Inventory {
    boot: u64,
    revision: u64,
    announced: u64,
    complete: bool,
    nodes: Vec<Node>,
    high: [u64; MAX_FUNCTIONS],
    birth: [u64; MAX_FUNCTIONS],
}
impl Inventory {
    pub fn new(boot: u64) -> Result<Self> {
        if boot == 0 {
            return Err(Error::Format);
        }
        Ok(Self {
            boot,
            revision: 0,
            announced: 0,
            complete: false,
            nodes: Vec::new(),
            high: [0; MAX_FUNCTIONS],
            birth: [0; MAX_FUNCTIONS],
        })
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn ready(&self) -> bool {
        self.complete && self.announced <= self.revision
    }
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }
    pub fn node(&self, k: Slot) -> Result<&Node> {
        self.nodes
            .binary_search_by_key(&k, |n| n.slot)
            .map(|i| &self.nodes[i])
            .map_err(|_| Error::Stale)
    }
    pub fn attachment(&self, k: Slot) -> Result<Attachment> {
        let n = self.node(k)?;
        Ok(Attachment {
            boot: self.boot,
            controller: n.controller,
            slot: k,
            appearance: n.appearance,
            topology: self.birth[k.0 as usize],
        })
    }
    pub fn notify(&mut self, rev: u64) -> Result<bool> {
        if rev <= self.announced {
            return Err(Error::Stale);
        }
        let gap = Some(rev) != self.announced.checked_add(1);
        self.announced = rev;
        Ok(gap)
    }
    pub fn refresh(
        &mut self,
        rev: u64,
        complete: bool,
        input: impl IntoIterator<Item = Node>,
    ) -> Result<u128> {
        if rev < next(self.revision)? || rev < self.announced {
            return Err(Error::Stale);
        }
        let mut nodes = Vec::<Node>::new();
        let input = input.into_iter();
        nodes
            .try_reserve_exact(input.size_hint().0.min(MAX_FUNCTIONS))
            .map_err(|_| Error::Memory)?;
        for mut n in input {
            n.validate()?;
            let high = self.high[n.slot.0 as usize];
            if n.appearance < high
                || n.appearance == high
                    && !self.node(n.slot).is_ok_and(|old| {
                        old.same_place(&n) && old.identity == n.identity && old.address == n.address
                    })
            {
                return Err(Error::Stale);
            }
            if nodes.iter().any(|other| {
                other.slot == n.slot || n.same_place(other) || n.usb_config_conflict(other)
            }) {
                return Err(Error::Conflict);
            }
            crate::push(&mut nodes, n, MAX_FUNCTIONS)?;
        }
        nodes.sort_unstable_by_key(|n| n.slot);
        let mask = nodes.iter().fold(0, |bits, n| bits | n.slot.bit());
        if complete {
            if nodes.iter().any(|n| {
                (n.requires() | n.aliases) & !mask != 0
                    || n.ownership == Ownership::Parent && n.parent.is_none()
            }) {
                return Err(Error::Incomplete);
            }
            let mut sorted = 0;
            while sorted != mask {
                let before = sorted;
                for n in &nodes {
                    if n.requires() & !sorted == 0 {
                        sorted |= n.slot.bit()
                    }
                }
                if sorted == before {
                    return Err(Error::Conflict);
                }
            }
        }
        let old_mask = self.nodes.iter().fold(0, |m, n| m | n.slot.bit());
        let mut changed = old_mask ^ mask;
        for n in &nodes {
            if self.node(n.slot) != Ok(n) {
                changed |= n.slot.bit();
            }
            if self.high[n.slot.0 as usize] != n.appearance {
                self.birth[n.slot.0 as usize] = rev;
            }
            self.high[n.slot.0 as usize] = n.appearance;
        }
        // Une variation des dépendances ou d'un périmètre commun périme aussi
        // le groupe concerné, même si son numéro sur le bus reste identique.
        loop {
            let before = changed;
            for n in self.nodes.iter().chain(nodes.iter()) {
                if changed & n.slot.bit() != 0 && (n.reset != 0 || n.irq_scope != 0) {
                    for peer in self.nodes.iter().chain(nodes.iter()) {
                        if n.reset != 0 && n.reset == peer.reset
                            || n.irq_scope != 0 && n.irq_scope == peer.irq_scope
                        {
                            changed |= peer.slot.bit();
                        }
                    }
                }
                if n.requires() & changed != 0 {
                    changed |= n.slot.bit();
                }
                if (n.slot.bit() | n.aliases) & changed != 0 {
                    changed |= n.slot.bit() | n.aliases;
                }
            }
            if before == changed {
                break;
            }
        }
        for n in &nodes {
            if changed & n.slot.bit() != 0 {
                self.birth[n.slot.0 as usize] = rev;
            }
        }
        self.nodes = nodes;
        self.complete = complete;
        self.revision = rev;
        self.announced = rev;
        Ok(changed)
    }
    pub fn scope(&self, k: Slot) -> Result<Scope> {
        if !self.ready() {
            return Err(Error::Incomplete);
        }
        let root = self.node(k)?;
        let mut members = k.bit();
        loop {
            let before = members;
            for n in &self.nodes {
                let peers = n.aliases | n.slot.bit();
                if peers & members != 0 {
                    members |= peers;
                }
            }
            if members == before {
                break;
            }
        }
        let mut resets = 0;
        let mut deps = 0;
        for n in self.nodes.iter().filter(|n| n.slot.bit() & members != 0) {
            if n.controller != root.controller
                || n.ownership != Ownership::Independent
                || !n.provider_trusted
                || matches!(n.identity, Bus::Unknown)
            {
                return Err(Error::Denied);
            }
            deps |= n.requires();
            for p in &self.nodes {
                if n.reset != 0 && p.reset == n.reset
                    || n.irq_scope != 0 && p.irq_scope == n.irq_scope
                {
                    resets |= p.slot.bit();
                }
            }
        }
        Ok(Scope {
            members,
            reset_peers: resets & !members,
            dependencies: deps & !members,
        })
    }
    pub fn bytes(&self) -> usize {
        size_of::<Self>()
            + self.nodes.capacity() * size_of::<Node>()
            + self.nodes.iter().map(Node::external_bytes).sum::<usize>()
    }
}
