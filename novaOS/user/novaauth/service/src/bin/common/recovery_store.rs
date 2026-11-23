//! Client du port privé NovaFS : aucun objet ou chemin arbitraire.
use crate::{common,runtime as rt};
pub struct Storage {port:u64,sequence:u64,operation:[u8;16]}
impl Storage {
    pub fn new(operation:[u8;16])->rt::Result<Self> {
        let port=rt::config(9);if port==0 {return Err(rt::DENIED);}
        Ok(Self {port,sequence:u64::from_le_bytes(*b"NATRX001"),operation})
    }
    fn request_private(&mut self,op:u8,table:usize,offset:usize,data:&[u8])->rt::Result<[u8;128]> {
        if data.len()>96 || table>2 || offset>16384 {return Err(rt::BAD_ARGUMENT);}
        let mut q=[0;128];q[0]=op;q[1]=table as u8;q[8..24].copy_from_slice(&self.operation);
        q[24..28].copy_from_slice(&(offset as u32).to_le_bytes());
        q[28..30].copy_from_slice(&(data.len() as u16).to_le_bytes());q[32..32+data.len()].copy_from_slice(data);
        self.sequence=self.sequence.checked_add(1).ok_or(rt::LIMIT)?;
        let a=common::rpc(self.port,self.port,self.sequence,&q)?;
        match common::number(&a) {rt::OK=>Ok(a),e=>Err(e)}
    }
    pub fn begin(&mut self,head:&[u8],lengths:[u32;3])->rt::Result<()> {
        if head.len()!=144 {return Err(rt::BAD_ARGUMENT);}
        let mut descriptor=[0;156];descriptor[..144].copy_from_slice(head);
        for (i,n) in lengths.iter().enumerate() {descriptor[144+4*i..148+4*i].copy_from_slice(&n.to_be_bytes());}
        for (i,part) in descriptor.chunks(96).enumerate() {self.request_private(1,0,i*96,part)?;}
        Ok(())
    }
    pub fn begin_next(&mut self,previous:&[u8;32],operation:[u8;16],head:&[u8],lengths:[u32;3])->rt::Result<()> {
        if operation==self.operation || operation==[0;16] {return Err(rt::DENIED);}
        self.request_private(6,0,0,previous)?;
        self.operation=operation;
        self.begin(head,lengths)
    }
    pub fn append(&mut self,table:usize,offset:usize,data:&[u8])->rt::Result<()> {
        for (i,part) in data.chunks(96).enumerate() {self.request_private(2,table,offset+i*96,part)?;}
        Ok(())
    }
    pub fn seal(&mut self)->rt::Result<()> {
        let a=self.request_private(3,0,0,&[])?;if a[8]!=3 {return Err(rt::REFUSED);}Ok(())
    }
    pub fn read(&mut self,table:usize,offset:usize,data:&mut [u8])->rt::Result<usize> {
        if data.len()>96 {return Err(rt::LIMIT);}
        let a=self.request_private(4,table,offset,&[0;96][..data.len()])?;
        let n=common::number(&a[8..]) as usize;
        if n==0 || n>data.len() {return Err(rt::REFUSED);}
        data[..n].copy_from_slice(&a[32..32+n]);Ok(n)
    }
}

#[cfg(feature="membership-r2-guest")]
impl Storage {
    /// L'instance provient de la page de démarrage écrite par le noyau QEMU,
    /// liée à la conversation existante. Aucun identifiant choisi par le CLI.
    pub fn membership_instance(&self)->rt::Result<[u8;32]> {
        let mut id=[0;32];
        for (i,chunk) in id.chunks_exact_mut(8).enumerate() {
            chunk.copy_from_slice(&rt::config(23+i).to_le_bytes());
        }
        if id==[0;32] {Err(rt::DENIED)} else {Ok(id)}
    }
    pub fn membership_close(&mut self, request:&novaos_recovery::membership_effectors::VfsClosure)
        ->rt::Result<novaos_recovery::membership_effectors::VfsFacts> {
        if request.bytes()[72..104]!=self.membership_instance()? {return Err(rt::DENIED);}
        self.sequence=self.sequence.checked_add(1).ok_or(rt::LIMIT)?;
        let reply=common::rpc(self.port,self.port,self.sequence,request.bytes())?;
        if common::number(&reply)!=rt::OK {return Err(common::number(&reply));}
        request.verify(&reply).map_err(|_|rt::REFUSED)
    }
}
