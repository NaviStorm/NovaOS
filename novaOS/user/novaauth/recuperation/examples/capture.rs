//! Construction de témoins chiffrés de laboratoire avec les seules clés publiques.
use novaos_recovery::{Capture,Encoder,CHUNK,MAX_OBJECT};
use rand_core::{CryptoRng,RngCore};
use std::io::{Read,Write};
use zeroize::Zeroizing;
struct Random(std::fs::File);
impl RngCore for Random {
    fn fill_bytes(&mut self,d:&mut [u8]) {self.0.read_exact(d).expect("source aléatoire");}
    fn next_u32(&mut self)->u32 {let mut b=[0;4];self.fill_bytes(&mut b);u32::from_le_bytes(b)}
    fn next_u64(&mut self)->u64 {let mut b=[0;8];self.fill_bytes(&mut b);u64::from_le_bytes(b)}
    fn try_fill_bytes(&mut self,d:&mut [u8])->Result<(),rand_core::Error> {
        self.0.read_exact(d).map_err(|_|rand_core::Error::from(core::num::NonZeroU32::new(rand_core::Error::CUSTOM_START).unwrap()))
    }
}
impl CryptoRng for Random {}
fn main()->Result<(),Box<dyn std::error::Error>> {
    let mut source=std::io::stdin().lock();
    let mut metadata=[0;16+16+144+8+64+12];
    source.read_exact(&mut metadata)?;
    let mut random=Random(std::fs::File::open("/dev/urandom")?);
    let mut backup=[0;32];random.fill_bytes(&mut backup);
    let head:[u8;144]=metadata[32..176].try_into()?;
    let capture=Capture {server:metadata[..16].try_into()?,domain:metadata[16..32].try_into()?,
        backup,generation:u64::from_be_bytes(head[88..96].try_into()?),head,
        key_epoch:u64::from_be_bytes(metadata[176..184].try_into()?),
        recipients:[metadata[184..216].try_into()?,metadata[216..248].try_into()?]};
    let mut lengths=[0u32;3];
    for (i,n) in lengths.iter_mut().enumerate() {*n=u32::from_be_bytes(metadata[248+4*i..252+4*i].try_into()?);}
    if lengths.into_iter().any(|n|n==0 || n as usize>MAX_OBJECT) {return Err("taille objet".into());}
    let mut encoder=Encoder::new(&capture,lengths,&mut random).map_err(|_|"capture refusée")?;
    let mut ciphertext=Vec::new();
    for length in lengths {
        let mut remaining=length as usize;
        while remaining!=0 {
            let n=remaining.min(CHUNK);
            let mut block=Zeroizing::new([0;CHUNK]);
            source.read_exact(&mut block[..n])?;
            let tag=encoder.seal(&mut block[..n]).map_err(|_|"fragment refusé")?;
            ciphertext.extend_from_slice(&((n+16) as u32).to_be_bytes());
            ciphertext.extend_from_slice(&block[..n]);ciphertext.extend_from_slice(&tag);
            remaining-=n;
        }
    }
    if source.read(&mut [0;1])?!=0 {return Err("entrée surnuméraire".into());}
    let manifest=encoder.finish().map_err(|_|"capture incomplète")?;
    let mut output=std::io::stdout().lock();
    output.write_all(&manifest)?;output.write_all(&ciphertext)?;output.flush()?;
    Ok(())
}
