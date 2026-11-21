//! Effecteur de préparation, réservé à NovaFS dans le profil QEMU.
//! Ne décide ni des droits ni de la tête. Aucun chemin ou objet libre.
//! Une erreur d'écriture ferme l'opération ; aucune répétition automatique.
use super::*;
use crate::recuperation::{decode_head,SnapshotDigest};

pub const FRAGMENT:usize=96;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Phase { Preparing, Uncertain, Sealed }
pub struct Store {
    head:Head,
    request:Id,
    lengths:[u32;3],
    table:usize,
    offset:u32,
    phase:Phase,
}
impl Store {
    /// Le port privé authentifie l'instance du registre et sérialise le domaine.
    /// Cette préparation reste invisible à tous les lecteurs opérationnels.
    pub fn begin<D:Device>(fs:&mut Engine<D>, head_bytes:&[u8], request:Id, lengths:[u32;3],domain:Id)->Result<Self> {
        let head=decode_head(head_bytes)?;
        if request==[0;16] {return Err(Fault::BadInput);}
        shape(fs,&head,domain)?;
        SnapshotDigest::new(head_bytes,lengths).map_err(|_|Fault::Limit)?;
        Ok(Self {head,request,lengths,table:0,offset:0,phase:Phase::Preparing})
    }
    pub fn phase(&self)->Phase {self.phase}
    pub fn request(&self)->Id {self.request}
    pub fn head(&self)->Head {self.head}
    pub fn cursor(&self)->(usize,u32) {(self.table,self.offset)}
    pub fn append<D:Device>(&mut self,fs:&mut Engine<D>,request:Id,table:usize,offset:u32,data:&[u8])->Result<()> {
        if request!=self.request || self.phase!=Phase::Preparing {return Err(Fault::Denied);}
        if table!=self.table || table>=3 || offset!=self.offset || data.is_empty() ||
            data.len()>FRAGMENT || data.len()>self.lengths[table].saturating_sub(offset) as usize {
            return Err(Fault::BadInput);
        }
        // Avant la première publication, toute erreur est désormais d'issue
        // potentiellement incertaine. L'appelant consultera, sans rejouer.
        self.phase=Phase::Uncertain;
        if offset==0 {
            let mut prefix=header(&self.head,table);
            prefix.extend(self.lengths[table].to_le_bytes());
            fs.prepare_write_id(self.head.objects[table],slot(&self.head),&prefix)?.commit()?;
        }
        fs.prepare_write_id(self.head.objects[table],slot(&self.head)+53+u64::from(offset),data)?.commit()?;
        self.offset+=data.len() as u32;
        if self.offset==self.lengths[table] {self.table+=1;self.offset=0;}
        self.phase=Phase::Preparing;
        Ok(())
    }
    /// Relecture seule, notamment après UNCERTAIN. N'effectue aucune mutation.
    /// Le reçu d'autorité et l'activation relèvent toujours du registre.
    pub fn seal<D:Device>(&mut self,fs:&Engine<D>,request:Id)->Result<Head> {
        if request!=self.request {return Err(Fault::Denied);}
        shape(fs,&self.head,self.head.domain)?;
        let parts=read_parts(fs,&self.head)?;
        if parts.each_ref().map(|p|p.len() as u32)!=self.lengths {
            return Err(Fault::StateMismatch);
        }
        // Les trois encodages sont valides ensemble, mais aucun droit n'est
        // activé par ce contrôle de stockage.
        load(fs,&self.head)?;
        self.phase=Phase::Sealed;
        Ok(self.head)
    }
    /// Fragments de relecture du triplet scellé, jamais une lecture arbitraire.
    pub fn read<D:Device>(&self,fs:&Engine<D>,request:Id,table:usize,offset:u32,out:&mut [u8])->Result<usize> {
        if request!=self.request || self.phase!=Phase::Sealed {return Err(Fault::Denied);}
        if table>=3 || offset>=self.lengths[table] || out.is_empty() || out.len()>FRAGMENT {
            return Err(Fault::BadInput);
        }
        let n=out.len().min((self.lengths[table]-offset) as usize);
        let bytes=fs.inspect_id(self.head.objects[table],slot(&self.head)+53+u64::from(offset),n)?;
        if bytes.len()!=n {return Err(Fault::Corrupt);}
        out[..n].copy_from_slice(&bytes);
        Ok(n)
    }
}
