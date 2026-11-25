//! Superviseur du profil Q35 PIO à deux domaines. Propriétaire exclusif de la QI.
//! Le scénario fermé ne donne aucun handle ni adresse physique aux pilotes.
use super::*;

const NIC: u16 = 56;
const NIC_DID: u16 = 3;
const NIC_IOVA: u64 = 0x1000_0000;
const NIC_CTX: usize = NIC as usize * 16;
const FRAMES: usize = 26;

#[derive(Clone, Copy)]
struct Operation {
    sid: u16,
    did: u16,
    owner: u64,
    generation: u64,
    publication: bool,
}

struct Receipt {
    operation: Operation,
    cookie: u32,
    tail: u16,
}

pub(super) struct Supervisor {
    enabled: bool,
    frames: [Frame; FRAMES],
    generation: u64,
    owner: u64,
    published: bool,
    requested: Option<Operation>,
    pending: Option<Receipt>,
}

impl Supervisor {
    pub(super) fn new() -> Self {
        Self {
            enabled: crate::boot::scenario().starts_with(b"net-q35-duo-"),
            frames: [EMPTY; FRAMES], generation: 0, owner: 0, published: false,
            requested: None, pending: None,
        }
    }

    pub(super) fn delivery_ok(&self, regs:Region)->bool {
        if !self.enabled {return true;}
        let port=super::super::pci_ports::port;
        // NVMe reste en polling, INTx masquée et MSI-X désactivée. Le NIC sans MSI/MSI-X
        // est identifié avant admission ; le chemin INTx contrôle la broche et la route IOAPIC.
        format::interrupt_state(regs.r32(0x1c), regs.r64(0xb8)) &&
            port(SID,0,4,None)==0x0010_1b36 && port(SID,4,2,None)&0x400!=0 &&
            port(SID,0x40,2,None)==0x8011 && port(SID,0x42,2,None)&0x8000==0 &&
            port(NIC,0,4,None)==0x1041_1af4 && port(NIC,0x3d,1,None)==1
    }

    pub(super) fn descriptors(&mut self, original: (u64,u64), mode: Invalidation,
        lease: Option<Lease>, _state: State, cookie: u32, tail: u16) -> Result<(u64,u64)> {
        if !self.enabled {return Ok(original);}
        if self.pending.is_some() || mode != Invalidation::Global {return Err(Error::Quarantine);}
        let operation = match self.requested.take() {
            Some(op) => op,
            None if cookie == 1 && lease.is_none() => return Ok(original),
            None => return Err(Error::Quarantine),
        };
        self.pending = Some(Receipt { operation, cookie, tail: (tail + 48) % 4096 });
        Ok((0x31 | (u64::from(operation.did)<<16) | (u64::from(operation.sid)<<32),
            0xe2 | (u64::from(operation.did)<<16)))
    }

    pub(super) fn completed(&mut self, cookie:u32, tail:u16, hardware_tail:u64) -> Result<()> {
        if !self.enabled {return Ok(());}
        if cookie == 1 && self.pending.is_none() && tail == 48 && hardware_tail == 48 {return Ok(());}
        let expected = self.pending.as_ref().ok_or(Error::Quarantine)?;
        if expected.cookie != cookie || expected.tail != tail || hardware_tail != u64::from(tail) {
            return Err(Error::Quarantine);
        }
        let op = expected.operation;
        hal::log(format_args!("DUO WAIT sid={} did={} instance={} generation={} publication={} cookie={cookie} iqt={tail}",op.sid,op.did,op.owner,op.generation,op.publication));
        self.pending = None;
        Ok(())
    }

    pub(super) fn nic_context(&self, index:usize)->u64 {
        if !self.published {return 0;}
        match index {
            112 => self.frames[0].pa | 1,
            113 => (u64::from(NIC_DID)<<8) | 2,
            _ => 0,
        }
    }

    fn entry(&self, level:usize, index:usize)->u64 {
        match level {
            0..=2 if index == ((NIC_IOVA >> [39,30,21][level]) & 511) as usize => self.frames[level+1].pa | 3,
            3 if index < 22 => self.frames[4+index].pa | if matches!(index, 2 | 5 | 6..=13) {2} else {1},
            _ => 0,
        }
    }

    pub(super) fn verify_nic(&self, service:&[Frame; ANCHORS], disk:&[Option<Mapping>;SLOTS])->bool {
        if !self.enabled {return true;}
        if service[1].mem.r64(NIC_CTX) != self.nic_context(112) ||
            service[1].mem.r64(NIC_CTX+8) != self.nic_context(113) {return false;}
        if !self.published {return true;}
        self.frames.iter().enumerate().all(|(i,f)| {
            f.pa != 0 && f.pa < 1<<48 && f.pa % PAGE == 0 &&
            self.frames[..i].iter().chain(service.iter()).all(|other| other.pa != f.pa) &&
            disk.iter().flatten().all(|m| m.frame.pa != f.pa)
        }) && (0..4).all(|level| (0..512).all(|i| self.frames[level].mem.r64(i*8) == self.entry(level,i)))
    }

    fn request(&mut self, op:Operation) -> Result<()> {
        if !self.enabled || self.requested.is_some() || self.pending.is_some() || op.owner == 0 || op.generation == 0 {
            return Err(Error::Quarantine);
        }
        self.requested = Some(op);
        Ok(())
    }
}

impl Engine {
    pub(super) fn request_disk(&mut self, lease:Lease, publication:bool)->Result<()> {
        if !self.duo.enabled {return Ok(());}
        self.duo.request(Operation {sid:SID, did:DID, owner:lease.owner(), generation:lease.epoch(), publication})
    }
    fn nic_wait(&mut self, publication:bool, lost:bool)->Result<()> {
        let op=Operation {sid:NIC, did:NIC_DID, owner:self.duo.owner, generation:self.duo.generation, publication};
        self.duo.request(op)?;
        self.submit_invalidation(Invalidation::Global)?;
        if lost {return Err(Error::Quarantine);}
        self.confirm()
    }
    fn closed_nic(&self)->bool {
        super::super::pci_ports::port(NIC,0,4,None)==0x1041_1af4 &&
        super::super::pci_ports::port(NIC,4,2,None)&4==0 && !self.fault_pending() && self.running()
    }
    pub(in crate::hardware) fn prepare_nic(&mut self, owner:u64, lease:Lease, layout:NvmeLayout)->Result<()> {
        if !self.duo.enabled || owner==0 || self.in_queue || self.duo.published ||
           self.duo.frames.iter().any(|f|f.pa!=0) || !self.closed_nic() || !self.exact_nvme(lease,layout) {
            return Err(Error::Denied);
        }
        let next=self.duo.generation.checked_add(1).ok_or(Error::Denied)?;
        for i in 0..FRAMES {
            let f=match Frame::allocate() {
                Ok(f)=>f, Err(_)=>{
                    for frame in &mut self.duo.frames[..i] {free(*frame); *frame=EMPTY;}
                    return Err(Error::Memory)
                }
            };
            if f.pa==0 || f.pa%PAGE!=0 || f.pa>=1<<48 ||
                self.service.iter().chain(self.duo.frames[..i].iter()).any(|other|other.pa==f.pa) ||
                self.pages.iter().flatten().any(|m|m.frame.pa==f.pa) {return Err(Error::Quarantine);}
            for index in 0..512 {f.mem.w64(index*8,0);}
            self.duo.frames[i]=f;
        }
        for level in 0..4 {
            for i in 0..512 {self.duo.frames[level].mem.w64(i*8,self.duo.entry(level,i));}
        }
        // Publier toutes les pages privées avant la mise en présence du contexte.
        if self.cache_line!=0 {
            for frame in &self.duo.frames {
                for offset in (0..4096).step_by(self.cache_line) {
                    unsafe {core::arch::x86_64::_mm_clflush((frame.mem.base()+offset) as *const u8);}
                }
            }
        }
        barrier();
        self.duo.owner=owner;
        self.duo.generation=next;
        self.duo.published=true;
        self.service[1].mem.w64(NIC_CTX+8,self.duo.nic_context(113));
        barrier();
        self.service[1].mem.w64(NIC_CTX,self.duo.nic_context(112));
        self.nic_wait(true,false)?;
        if !self.exact_nvme(lease,layout) || !self.closed_nic(){return Err(Error::Quarantine);}
        Ok(())
    }

    // Uniquement pour le témoin fermé. L'API de cet essai n'expose aucun moyen d'armer le NIC.
    pub(in crate::hardware) fn remove_unstarted_nic(&mut self, lost:bool)->Result<()> {
        if !self.duo.published || !self.closed_nic() || self.in_queue ||
            !self.duo.verify_nic(&self.service,&self.pages) {return Err(Error::Quarantine);}
        self.service[1].mem.w64(NIC_CTX,0);
        barrier();
        self.service[1].mem.w64(NIC_CTX+8,0);
        self.nic_wait(false,lost)?;
        if !self.closed_nic() || self.service[1].mem.r64(NIC_CTX)!=0 || self.service[1].mem.r64(NIC_CTX+8)!=0 {
            return Err(Error::Quarantine);
        }
        self.duo.published=false;
        for frame in &mut self.duo.frames {free(*frame); *frame=EMPTY;}
        Ok(())
    }
    pub(in crate::hardware) fn trace_duo(&self) {
        hal::log(format_args!("DUO tables root={:#x} qi={:#x} wait={:#x} iqt={} cookie={} nic={} nvme={} en_attente={}",
            self.service[0].pa,self.service[6].pa,self.service[7].pa,self.tail,self.cookie,self.duo.published,self.context(),self.in_queue));
    }
}
