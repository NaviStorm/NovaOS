//! Port de données existant de nsh ; décision séparée sur R -> NovaFS.
use super::*;
use novaos_identity::policy::{Origin,Permission};
#[derive(Default)]
pub(super) struct Exchange {pub pending:Option<(u64,[u8;80])>,context:Option<[u8;120]>}
fn compact<const N:usize>(frame:&Frame)->rt::Result<[u8;N]> {
    if N>128 || frame[N..].iter().any(|b|*b!=0){return Err(rt::BAD_ARGUMENT);}
    frame[..N].try_into().map_err(|_|rt::BAD_ARGUMENT)
}
impl Exchange {
    pub(super) fn enqueue(&mut self,lane:usize,last:&mut u64,tag:u64,data:Frame)->rt::Result<()> {
        if lane!=0 || tag<=*last {return Err(rt::DENIED);}
        if self.pending.is_some() || self.context.is_some(){return Err(rt::WOULD_BLOCK);}
        if &data[..8]!=b"NXMGDAT1" || data[8..40]==[0;32] || data[40..48]==[0;8]
            || !matches!(data[72],1|2) || !matches!(data[73],1|3) || data[74..]!=[0;54]
            || (data[72]==1 && data[64..72]!=[0;8]) || (data[72]==2 && data[64..72]==[0;8]) {
            return Err(rt::BAD_ARGUMENT);
        }
        *last=tag;self.pending=Some((tag,compact(&data)?));Ok(())
    }
}
impl Exchange {
    /// Le fait et la réponse remplacent les buffers de requête : aucun octet
    /// statique supplémentaire. Une consultation ne rejoue jamais les E/S.
    fn remembered(&self,digest:&[u8])->rt::Result<Frame> {
        let mut fact=[0;128];fact[..120].copy_from_slice(&self.context.ok_or(rt::WOULD_BLOCK)?);
        if &fact[8..16]!=b"NXMGIOA1" || &fact[16..48]!=digest {return Err(rt::DENIED);}
        Ok(fact)
    }
    fn deliver(&mut self)->rt::Result<()> {
        if let Some((tag,stored))=self.pending {
            let mut output=[0;128];output[..80].copy_from_slice(&stored);
            match rt::send_frame(rt::config(5),tag,&output,0,false) {
                Ok(_) | Err(rt::CLOSED|rt::REVOKED)=>self.pending=None,
                Err(_)=>return Err(w::UNCERTAIN),
            }
        }
        Ok(())
    }
}
fn n(b:&[u8],i:usize)->u64{u64::from_le_bytes(b[i..i+8].try_into().unwrap())}
fn identity(at:usize)->[u8;32]{let mut b=[0;32];for(i,c)in b.chunks_exact_mut(8).enumerate(){c.copy_from_slice(&exec_files_lab::config(at+i).to_le_bytes());}b}
pub(super) fn control(s:&mut Session<'_,'_>,q:&Frame)->rt::Result<Frame>{
    if matches!(&q[..8],b"NXMGRES1"|b"NXMGREL1") {
        if q[8..40]==[0;32] || q[40..]!=[0;88]{return Err(rt::BAD_ARGUMENT);}
        // REL est un nettoyage idempotent, jamais une preuve de réussite.
        if &q[..8]==b"NXMGREL1" && s.membership_io.context.is_none() && s.membership_io.pending.is_none(){return Ok([0;128]);}
        let fact=s.membership_io.remembered(&q[8..40])?;
        s.membership_io.deliver()?;
        if &q[..8]==b"NXMGREL1" {s.membership_io.context=None;return Ok([0;128]);}
        return Ok(fact);
    }

    if &q[..8]==b"NXMGCTX2" {
        if q[8..24]==[0;16] || q[24..56]==[0;32] || q[56..88]!=identity(472) || q[120..]!=[0;8] {return Err(rt::DENIED);}
        if s.membership_io.context.is_some(){return Err(rt::WOULD_BLOCK);}
        s.membership_io.context=Some(compact(q)?);return Ok([0;128]);
    }
    let mut c=[0;128];c[..120].copy_from_slice(&s.membership_io.context.take().ok_or(rt::DENIED)?);
    let (tag,stored)=s.membership_io.pending.take().ok_or(rt::WOULD_BLOCK)?;
    let mut data=[0;128];data[..80].copy_from_slice(&stored);
    let mut output=[0;128];output[..8].copy_from_slice(b"NXMGOUT1");
    let r=(||{
        if !matches!(&q[..8],b"NXMGGO01"|b"NXMGDEN1") || n(q,8)!=tag
            || data[8..40]!=c[24..56] || data[40..48]!=q[16..24]
            || data[48..64]!=q[32..48] || data[64..72]!=q[24..32]
            || data[72]!=q[72] || data[73]!=q[74] || !matches!(q[74],1|3) || data[74..]!=[0;54] || q[75..80]!=[0;5] || q[104..]!=[0;24] {return Err(rt::DENIED);}
        if &q[..8]==b"NXMGDEN1" {
            // Le témoin de réutilisation vérifie aussi le handle réel, pas
            // seulement le refus de permission fraîche dans R.
            if q[72]==2 {
                let holder=auth_directory::Holder{subject:c[8..24].try_into().unwrap(),session:c[24..56].try_into().unwrap(),port:rt::config(4)};
                if s.directory.check(holder,n(q,24),q[74]).err()!=Some(rt::REVOKED){return Err(rt::REFUSED);}
            }
            let e=n(q,80);return Err(if matches!(e,3|5|10|12|22){e}else{rt::DENIED});
        }
        if s.failed.is_some() || s.pending.is_some(){return Err(rt::REFUSED);}
        s.mount_volume()?;
        let fs=s.fs.as_ref().ok_or(rt::CLOSED)?;
        let mark=fs.lookup_at(fs.root_id(),"\u{1f}NXScope").map_err(error)?;
        if mark.kind!=Kind::File || mark.size!=40{return Err(rt::DENIED);}
        let seal=fs.inspect_id(mark.id,0,40).map_err(error)?;
        if &seal[..8]!=b"NXScope1" || seal[8..24]!=c[8..24] || n(&seal,24)!=n(q,16)
            || novaos_identity::recuperation_view::seal_digest(&seal)!=c[88..120] {return Err(rt::DENIED);}
        let object=n(q,16);let file=n(&seal,32);
        if fs.parent_name(file).map_err(error)?.0!=object{return Err(rt::DENIED);}
        let holder=auth_directory::Holder{subject:c[8..24].try_into().unwrap(),session:c[24..56].try_into().unwrap(),port:rt::config(4)};
        let origin=if q[32..48]==[0;16]{Origin::Direct}else{Origin::Group(q[32..48].try_into().unwrap())};
        let permission=Permission{origin,rights:q[73],rule_revision:n(q,48),membership_revision:n(q,56),image_revision:n(q,64)};
        let mut key=n(q,24);
        if q[72]==1 {
            if key!=0{return Err(rt::BAD_ARGUMENT);}
            key=s.directory.membership_grant(holder,0,q[74],permission)?;
            s.directory.leases.iter_mut().flatten().find(|l|l.key==key).ok_or(rt::REFUSED)?.object=object;
        }else if q[72]==2 {
            s.directory.check(holder,key,q[74])?;
            if s.directory.membership_permission(holder,key)?!=permission
                || !s.directory.leases.iter().flatten().any(|l|l.key==key && l.holder==holder && l.object==object){return Err(rt::REVOKED);}
            s.directory.membership_admit(holder,permission)?;
        }else{return Err(rt::BAD_ARGUMENT);}
        if q[74]&2!=0 {
            if !s.writable{return Err(rt::DENIED);}
            // Objet et droits ont été revalidés ; aucun chemin arbitraire et
            // aucune élévation de la permission. Données synthétiques du banc.
            let mut written=*b"A\tB\tC\nD\tE\tF\n";written[0]=q[32]^if q[72]==2{0x80}else{0};
            s.fs.as_mut().ok_or(rt::CLOSED)?.prepare_write_id(file,0,&written)
                .map_err(error)?.commit().map_err(error)?;
        }
        let revision=s.fs.as_ref().ok_or(rt::CLOSED)?.stat_id(file).map_err(error)?.revision;
        output[44..52].copy_from_slice(&revision.to_le_bytes());
        let bytes=s.fs.as_mut().ok_or(rt::CLOSED)?.read_access_id(file,0,12).map_err(error)?;
        if bytes.len()!=12{return Err(rt::REFUSED);}
        output[16..24].copy_from_slice(&key.to_le_bytes());output[24..32].copy_from_slice(&12u64.to_le_bytes());output[32..44].copy_from_slice(&bytes);
        Ok(())
    })();
    if let Err(code)=r {output[8..16].copy_from_slice(&code.to_le_bytes());}
    // Le registre ne reçoit aucun octet de fichier, seulement le fait corrélé.
    let mut a=[0;128];a[8..16].copy_from_slice(b"NXMGIOA1");
    a[16..48].copy_from_slice(&novaos_identity::recuperation_view::seal_digest(q));
    a[48..80].copy_from_slice(&identity(468));
    a[80..112].copy_from_slice(&novaos_identity::recuperation_view::seal_digest(&output));
    a[112..120].copy_from_slice(&output[8..16]);
    s.membership_io.context=Some(compact(&a)?);
    s.membership_io.pending=Some((tag,compact(&output)?));
    s.membership_io.deliver()?;
    Ok(a)
}

// Vérifié aussi par les compilations invitées des deux architectures.
const _:()=assert!(core::mem::size_of::<Exchange>()==224);
