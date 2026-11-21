//! Effecteur QEMU de lecture d'ELF : décision privée reçue du registre via 17.
//! Aucun Ledger, aucune mutation de politique, aucun droit d'installation.
use super::*;
pub(super) struct Upload {bytes:[u8;240],next:u64}
// Un seul message par chargement respecte le quota DEBUG historique.
// Compteurs : octets demandés vivants, blocs vivants, pages attribuées,
// pages rendues, appels malloc réussis (avant puis après), cache avant/après/budget.
struct MemoryMeasure {before:[usize;5],cache:[usize;3],reuse:[usize;3]}
impl Drop for MemoryMeasure {
    fn drop(&mut self) {
        let after=crate::heap::memory_snapshot();let mut values=[0usize;19];
        values[..5].copy_from_slice(&self.before);values[5..10].copy_from_slice(&after);values[10..13].copy_from_slice(&self.cache);values[13..16].copy_from_slice(&self.reuse);values[16..].copy_from_slice(&crate::heap::memory_reuse());
        // 19 compteurs de 48 bits : 254 octets, sous DEBUG_MAX=256.
        // Un dépassement est explicite ; aucune troncature silencieuse.
        if values.iter().any(|&v|v>>48!=0) {let message=b"NXMEM4 compteur hors borne";rt::call(rt::DEBUG_WRITE,message.as_ptr() as u64,message.len() as u64,0);return;}
        let mut line=[b' ';7+19*13];line[..7].copy_from_slice(b"NXMEM4 ");let hex=b"0123456789abcdef";
        for (column,value) in values.into_iter().enumerate() {for i in 0..12 {line[7+column*13+i]=hex[(value>>((11-i)*4))&15];}}
        rt::call(rt::DEBUG_WRITE,line.as_ptr() as u64,line.len() as u64,0);
    }
}
fn word(b:&[u8])->u64 {u64::from_le_bytes(b[..8].try_into().unwrap())}
impl Session<'_, '_> {
    pub(super) fn package_request(&mut self,q:&Frame,_a:&mut Frame)->rt::Result<()> {
        if self.failed.is_some() {return Err(rt::DENIED);}
        match &q[..8] {
            b"NXPKDROP"=>{self.upload=None;self.executable_request=None;Ok(())},
            b"NXPKBEGX"=>{
                if self.executable_request.is_some() || q[8..24]==[0;16] || q[24..56]==[0;32] || q[56..88]==[0;32] || q[88..104]==[0;16] || q[104..]!=[0;24] {return Err(rt::DENIED);}
                self.executable_request=Some(*q);self.upload=Some(Upload {bytes:[0;240],next:0});Ok(())
            },
            b"NXPKDSC1"=>{
                let request=self.executable_request.as_ref().ok_or(rt::DENIED)?;
                let upload=self.upload.as_mut().ok_or(rt::DENIED)?;let part=word(&q[24..]);
                if part>2 || part!=upload.next || q[8..24]!=request[88..104] || (part==2 && q[80..]!=[0;48]) {return Err(rt::DENIED);}
                let start=part as usize*96;let count=(240-start).min(96);
                upload.bytes[start..start+count].copy_from_slice(&q[32..32+count]);upload.next+=1;
                if part!=2 {return Ok(());}
                let d=self.upload.take().unwrap().bytes;let request=self.executable_request.take().unwrap();
                self.load_attested(&request,&d)
            },
            _=>Err(rt::DENIED)
        }
    }
    fn load_attested(&mut self,q:&Frame,d:&[u8;240])->rt::Result<()> {
        let mut memory=MemoryMeasure {before:crate::heap::memory_snapshot(),reuse:crate::heap::memory_reuse(),cache:[0;3]};
        if &d[..8]!=b"NXRES001" || d[8..40]!=q[56..88] || d[40..56]!=q[8..24] || d[56..72]==[0;16] || d[152..184]==[0;32] || d[234..]!=[0;6] {return Err(rt::DENIED);}
        #[cfg(target_arch="aarch64")] let machine=183u16;
        #[cfg(target_arch="x86_64")] let machine=62u16;
        if d[232..234]!=machine.to_le_bytes() {return Err(rt::DENIED);}
        let object=word(&d[120..]);let revision=word(&d[128..]);let offset=word(&d[136..]);let len=word(&d[144..]);
        if len==0 || len>novaos_identity::distribution::MAX_IMAGE as u64 {return Err(rt::LIMIT);}
        self.mount_volume()?;let fs=self.fs.as_ref().ok_or(rt::CLOSED)?;
        if d[80..96]!=fs.ids().fs || d[96..112]!=fs.ids().lv {return Err(rt::DENIED);}
        let cache=fs.cache_stats();memory.cache[0]=cache.charged_bytes;memory.cache[2]=cache.budget_bytes;
        let stat=fs.stat_id(object).map_err(error)?;
        if stat.kind!=Kind::File || stat.revision!=revision || offset.checked_add(len).is_none_or(|end|end>stat.size) {return Err(rt::REVOKED);}
        let mut bytes=Vec::new();bytes.try_reserve_exact(len as usize).map_err(|_|rt::LIMIT)?;
        while bytes.len()<len as usize {
            let count=(len as usize-bytes.len()).min(65536);
            let chunk=fs.inspect_id(object,offset+bytes.len() as u64,count).map_err(error)?;
            if chunk.len()!=count {return Err(rt::REFUSED);}bytes.extend_from_slice(&chunk);
        }
        if novaos_identity::recuperation_view::seal_digest(&bytes)!=d[152..184] {return Err(rt::REVOKED);}
        let tx=rt::config(19);let frame=|op:&[u8;8]| {let mut f=[0;128];f[..8].copy_from_slice(op);f[8..24].copy_from_slice(&q[88..104]);f};
        let mut f=frame(b"NXLDMETA");f[24..40].copy_from_slice(&d[40..56]);f[40..72].copy_from_slice(&d[8..40]);f[72..88].copy_from_slice(&d[80..96]);f[88..104].copy_from_slice(&d[56..72]);
        f[104..112].copy_from_slice(&d[72..80]);f[112..120].copy_from_slice(&d[120..128]);f[120..].copy_from_slice(&d[128..136]);rt::send_for(tx,1,&f,250)?;
        let mut f=frame(b"NXLDSIZE");f[24..32].copy_from_slice(&len.to_le_bytes());f[32..64].copy_from_slice(&d[152..184]);f[64..72].copy_from_slice(&d[112..120]);
        // Aucun droit sur un fichier utilisateur n'est dérivé de ce chargement.
        rt::send_for(tx,2,&f,250)?;
        for (index,chunk) in bytes.chunks(4096).enumerate() {
            let mut f=frame(b"NXLDDATA");f[24..32].copy_from_slice(&((index*4096) as u64).to_le_bytes());f[32..40].copy_from_slice(&(chunk.as_ptr() as u64).to_le_bytes());f[40..48].copy_from_slice(&(chunk.len() as u64).to_le_bytes());rt::send_for(tx,3,&f,250)?;
        }
        memory.cache[1]=fs.cache_stats().charged_bytes;
        rt::send_for(tx,4,&frame(b"NXLDSEAL"),250)
    }
}
