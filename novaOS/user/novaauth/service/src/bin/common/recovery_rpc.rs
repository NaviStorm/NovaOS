//! Multiplexage borné sur la conversation privée registre/signataire déjà déléguée.
use crate::{runtime as rt, incarnation};
use novaos_recovery::framing::{Header, Assembly, HEADER, MAX, PART, TAG};

pub const END:u16=1;
pub const BIND:u16=2;
pub const CURRENT:u16=10;
pub const KEYS:u16=11;
pub const BACKUP:u16=12;
pub const CHALLENGE:u16=13;
pub const APPROVE:u16=14;
pub const RESERVE:u16=15;
pub const DELIVER:u16=16;
pub const ACK:u16=17;
pub const ACTIVATE:u16=18;
pub const CIPHERTEXT:u16=19;
pub const ISSUE:u16=20;
pub const OBSERVE:u16=21;
pub const RETRIEVE:u16=22;

pub fn receive(port:u64, first:rt::ReceiveV1, out:&mut [u8;MAX], mut wait:impl FnMut()->rt::Result<u64>)->rt::Result<Header> {
    if first.kind!=1 || first.tag!=TAG || first.len!=128 {return Err(rt::BAD_ARGUMENT);}
    let header=Header::parse(&first.payload[..HEADER]).map_err(|_|rt::BAD_ARGUMENT)?;
    let mut assembly=Assembly::new(header).map_err(|_|rt::BAD_ARGUMENT)?;
    let target=&mut out[HEADER..HEADER+usize::from(header.size)];
    let mut m=first;
    loop {
        if m.kind!=1 || m.tag!=TAG || m.len!=128 {return Err(rt::BAD_ARGUMENT);}
        if assembly.push(&m.payload,target).map_err(|_|rt::BAD_ARGUMENT)? {break;}
        m=rt::receive_for(port,wait()?)?;
    }
    out[..HEADER].copy_from_slice(&header.encode().map_err(|_|rt::BAD_ARGUMENT)?);
    Ok(header)
}
pub fn send(port:u64, h:Header, data:&[u8], mut wait:impl FnMut()->rt::Result<u64>)->rt::Result<()> {
    if data.len()!=usize::from(h.size) {return Err(rt::BAD_ARGUMENT);}
    for offset in (0..data.len().max(1)).step_by(PART) {
        let frame=h.frame(offset,data).map_err(|_|rt::BAD_ARGUMENT)?;
        rt::send_for(port,TAG,&frame,wait()?)?;
    }
    Ok(())
}
pub struct Client {port:u64,sequence:u32,instance:[u8;32]}
impl Client {
    pub fn new(port:u64,instance:[u8;32])->Self {Self {port,sequence:0,instance}}
    pub fn exchange(&mut self,operation:u16,data:&[u8],answer:&mut [u8;MAX])->rt::Result<usize> {
        if data.len()>MAX-HEADER-64 {return Err(rt::LIMIT);}
        self.sequence=self.sequence.checked_add(1).ok_or(rt::LIMIT)?;
        let mut body=zeroize::Zeroizing::new([0;MAX-HEADER]);
        body[..16].copy_from_slice(&incarnation::SERVER);
        body[16..32].copy_from_slice(&incarnation::DOMAIN);
        body[32..64].copy_from_slice(&self.instance);
        body[64..64+data.len()].copy_from_slice(data);
        let h=Header {sequence:self.sequence,operation,size:(64+data.len()) as u16};
        send(self.port,h,&body[..usize::from(h.size)],||Ok(5000))?;
        let first=rt::receive_for(self.port,12000)?;
        let response=receive(self.port,first,answer,||Ok(2000))?;
        if response.sequence!=h.sequence || response.operation!=h.operation|0x8000 {return Err(rt::REFUSED);}
        Ok(usize::from(response.size))
    }
}
