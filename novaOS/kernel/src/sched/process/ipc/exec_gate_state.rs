//! Machine locale privée du profil QEMU. Toutes les transitions exigent Core verrouillé.
//! Une perte de réponse ou une expiration ne produit jamais une issue distante.
#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub enum Error { Refused, Busy, Invalid }
#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub enum Phase { Dormant, Requested, Admitted, Creating, Created, Cancelled, Settled }
#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub struct Gate {
    phase: Phase,
    operation: [u8;16],
    position: u64,
    child: [u8;32],
}
impl Gate {
    pub const fn new()->Self {Self {phase:Phase::Dormant,operation:[0;16],position:0,child:[0;32]}}
    pub fn phase(&self)->Phase {self.phase}
    pub fn operation(&self)->[u8;16] {self.operation}
    pub fn position(&self)->u64 {self.position}
    /// Liaison privée préalable, sans admission et sans mémoire supplémentaire.
    /// Appel uniquement après le reçu du descripteur et les liaisons vérifiées.
    pub fn bind(&mut self,op:[u8;16])->Result<(),Error> {
        if op==[0;16] {return Err(Error::Invalid);}
        if self.phase!=Phase::Dormant || self.operation!=[0;16] {return Err(Error::Refused);}
        self.operation=op;Ok(())
    }
    pub fn request(&mut self,op:[u8;16])->Result<(),Error> {
        if op==[0;16] {return Err(Error::Invalid);}
        if self.phase!=Phase::Dormant {return Err(Error::Refused);}
        if self.operation!=[0;16] && self.operation!=op {return Err(Error::Invalid);}
        self.operation=op;self.phase=Phase::Requested;Ok(())
    }
    pub fn admit(&mut self,op:[u8;16],position:u64)->Result<(),Error> {
        if position==0 || op!=self.operation {return Err(Error::Invalid);}
        if self.phase!=Phase::Requested {return Err(Error::Refused);}
        self.position=position;self.phase=Phase::Admitted;Ok(())
    }
    pub fn start(&mut self)->Result<(),Error> {
        if self.phase!=Phase::Admitted {return Err(Error::Refused);}
        self.phase=Phase::Creating;Ok(())
    }
    pub fn cancel_before_creation(&mut self)->Result<(),Error> {
        match self.phase {
            Phase::Dormant|Phase::Requested|Phase::Admitted=>{self.phase=Phase::Cancelled;Ok(())}
            Phase::Creating=>Err(Error::Busy),
            Phase::Cancelled=>Ok(()),
            _=>Err(Error::Refused),
        }
    }
    pub fn created(&mut self,child:[u8;32])->Result<(),Error> {
        if child==[0;32] {return Err(Error::Invalid);}
        if self.phase!=Phase::Creating {return Err(Error::Refused);}
        self.child=child;self.phase=Phase::Created;Ok(())
    }
    pub fn failed_after_cleanup(&mut self)->Result<(),Error> {
        if self.phase!=Phase::Creating {return Err(Error::Refused);}
        self.phase=Phase::Cancelled;Ok(())
    }
    pub fn terminal(&self)->Result<(u8,[u8;32]),Error> {
        match self.phase {
            Phase::Created=>Ok((1,self.child)),Phase::Cancelled=>Ok((2,[0;32])),
            _=>Err(Error::Busy),
        }
    }
    pub fn settle(&mut self,op:[u8;16],state:u8,child:[u8;32])->Result<(),Error> {
        if self.operation!=op || self.terminal()? != (state,child) {return Err(Error::Invalid);}
        self.phase=Phase::Settled;Ok(())
    }
    pub fn detachable(&self)->bool {matches!(self.phase,Phase::Dormant|Phase::Settled)}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]fn cancellation_is_irreversible_despite_delayed_reply() {
        let mut g=Gate::new();g.request([1;16]).unwrap();g.cancel_before_creation().unwrap();
        assert!(g.admit([1;16],1).is_err());assert!(g.start().is_err());
        assert_eq!(g.terminal(),Ok((2,[0;32])));
        g.settle([1;16],2,[0;32]).unwrap();assert!(g.detachable());
        assert!(g.request([2;16]).is_err());assert!(g.created([1;32]).is_err());
    }
    #[test]fn inflight_creation_cannot_be_cancelled_by_timeout() {
        let mut g=Gate::new();g.request([1;16]).unwrap();g.admit([1;16],9).unwrap();g.start().unwrap();
        assert_eq!(g.cancel_before_creation(),Err(Error::Busy));assert_eq!(g.terminal(),Err(Error::Busy));
        assert!(!g.detachable());g.created([7;32]).unwrap();
        assert_eq!(g.terminal(),Ok((1,[7;32])));assert!(g.start().is_err());
        assert!(g.settle([1;16],2,[0;32]).is_err());g.settle([1;16],1,[7;32]).unwrap();
    }
    #[test]fn cleanup_precedes_cancellation_proof() {
        let mut g=Gate::new();g.request([1;16]).unwrap();g.admit([1;16],9).unwrap();g.start().unwrap();
        assert!(g.terminal().is_err());g.failed_after_cleanup().unwrap();
        assert_eq!(g.terminal(),Ok((2,[0;32])));assert!(g.start().is_err());
    }
    #[test]fn concurrent_consumers_create_once() {
        use std::sync::{Arc,Mutex,Barrier};
        let gate=Arc::new(Mutex::new(Gate::new()));let start=Arc::new(Barrier::new(2));
        {let mut g=gate.lock().unwrap();g.request([1;16]).unwrap();g.admit([1;16],1).unwrap();}
        let threads:Vec<_>=(0..2).map(|_|{
            let g=gate.clone();let b=start.clone();std::thread::spawn(move||{b.wait();g.lock().unwrap().start()})
        }).collect();
        let result:Vec<_>=threads.into_iter().map(|t|t.join().unwrap()).collect();
        assert_eq!(result.iter().filter(|r|r.is_ok()).count(),1);
    }
    #[test]fn receipt_binding_and_unknown_outcome() {
        let mut g=Gate::new();assert!(g.admit([1;16],9).is_err());g.request([1;16]).unwrap();
        assert!(g.admit([2;16],9).is_err());assert!(g.admit([1;16],0).is_err());
        assert!(g.terminal().is_err());assert!(!g.detachable());
    }
}

#[cfg(test)] mod descriptor_tests {
 use super::*;
 #[test] fn dormant_binding_is_immutable_and_is_not_admission(){
  let mut g=Gate::new();g.bind([7;16]).unwrap();assert_eq!(g.phase(),Phase::Dormant);
  assert!(g.start().is_err());assert!(g.admit([7;16],1).is_err());
  assert!(g.bind([7;16]).is_err());assert!(g.bind([8;16]).is_err());
  assert!(g.request([8;16]).is_err());assert_eq!(g.operation(),[7;16]);
  g.request([7;16]).unwrap();g.cancel_before_creation().unwrap();
  assert!(g.admit([7;16],1).is_err());assert!(g.bind([8;16]).is_err());
  g.settle([7;16],2,[0;32]).unwrap();assert!(g.bind([8;16]).is_err());
 }
}
