//! Cœur P9 du profil de récupération. Le détenteur de l'écrivain NovaFS
//! sérialise toute intervention, de la préparation jusqu'à la réconciliation.
use super::*;

fn descriptor(head: &Head) -> Vec<u8> {
    let mut b = Vec::from(*b"NATRIP01");
    b.extend(head.domain);
    for id in head.volume {
        b.extend(id);
    }
    b.extend(head.recovery);
    for object in head.objects {
        b.extend(object.to_be_bytes());
    }
    b.extend(head.generation.to_be_bytes());
    b.extend(head.request);
    b.extend(head.digest);
    b
}

pub struct Triplet {
    head: Head,
    parts: [Vec<u8>; 3],
}

impl Triplet {
    pub fn head_bytes(head:&Head)->[u8;144] {descriptor(head)[8..].try_into().unwrap()}
    /// Lit uniquement les trois objets résolus dans le domaine provisionné.
    pub fn capture<D: Device>(fs: &Engine<D>, a: &impl Authority, domain: Id) -> Result<Self> {
        let head = a.current()?;
        shape(fs, &head, domain)?;
        let parts = read_parts(fs, &head)?;
        if a.current()? != head {
            return Err(Fault::Conflict);
        }
        Ok(Self { head, parts })
    }

    pub(crate) fn from_state(mut head:Head,state:State)->Result<Self> {
        let parts=[serde_json::to_vec(&state.users).map_err(|_|Fault::Limit)?,
                   serde_json::to_vec(&state.groups).map_err(|_|Fault::Limit)?,
                   serde_json::to_vec(&state.credentials).map_err(|_|Fault::Limit)?];
        if parts.iter().any(|p|p.is_empty() || p.len()>SLOT-53) {return Err(Fault::Limit);}
        head.digest=snapshot_digest(&head,&parts);
        Ok(Self {head,parts})
    }
    pub fn parts(&self)->[&[u8];3] {self.parts.each_ref().map(|p|p.as_slice())}
    pub fn head(&self) -> &Head {
        &self.head
    }

    // Encodage clair destiné exclusivement à l'enveloppe chiffrée de laboratoire.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = descriptor(&self.head);
        for p in &self.parts {
            out.extend((p.len() as u32).to_be_bytes());
            out.extend(p);
        }
        out
    }

    /// Appel du registre après déchiffrement. La tête attendue provient de la
    /// consultation authentifiée courante et non de l'enveloppe de sauvegarde.
    pub fn verify(current: Head, data: &[u8]) -> Result<Self> {
        if current.generation == 0 {
            return Err(Fault::NotEnrolled);
        }
        let mut rest = data
            .strip_prefix(descriptor(&current).as_slice())
            .ok_or(Fault::StateMismatch)?;
        let mut parts = [Vec::new(), Vec::new(), Vec::new()];
        for (i, p) in parts.iter_mut().enumerate() {
            let (len, tail) = rest.split_at_checked(4).ok_or(Fault::Corrupt)?;
            let n = u32::from_be_bytes(len.try_into().map_err(|_| Fault::Corrupt)?) as usize;
            if n > SLOT - header(&current, i).len() - 4 {
                return Err(Fault::Limit);
            }
            let (value, next) = tail.split_at_checked(n).ok_or(Fault::Corrupt)?;
            *p = Vec::from(value);
            rest = next;
        }
        if !rest.is_empty() || snapshot_digest(&current, &parts) != current.digest {
            return Err(Fault::StateMismatch);
        }
        Ok(Self {
            head: current,
            parts,
        })
    }

    /// Effecteur de laboratoire. Appel réservé à NovaFS sous réservation
    /// exclusive du triplet. La décision distante existe déjà ; aucune mutation
    /// d'autorité n'est envoyée, même si la réponse de stockage se perd.
    pub fn restore<D: Device>(
        &self,
        fs: &mut Engine<D>,
        authority: &impl Authority,
        domain: Id,
    ) -> Result<()> {
        if authority.current()? != self.head {
            return Err(Fault::StateMismatch);
        }
        shape(fs, &self.head, domain)?;
        match load(fs, &self.head) {
            Ok(_) => (),
            Err(Fault::StaleVolume | Fault::StateMismatch | Fault::Corrupt) => {
                for (i, p) in self.parts.iter().enumerate() {
                    let mut b = header(&self.head, i);
                    b.extend((p.len() as u32).to_le_bytes());
                    b.extend(p);
                    fs.prepare_write_id(self.head.objects[i], slot(&self.head), &b)?
                        .commit()?;
                }
                load(fs, &self.head)?;
            }
            Err(e) => return Err(e),
        }
        if authority.current()? != self.head {
            return Err(Fault::Conflict);
        }
        Ok(())
    }
}


pub use super::snapshot_digest::SnapshotDigest;

/// Tête canonique du manifeste de récupération : entiers en ordre réseau.
/// La confiance ne vient pas du décodage ; l'appelant vérifie la tête distante.
pub fn decode_head(bytes: &[u8]) -> Result<Head> {
    if bytes.len()!=144 {return Err(Fault::Corrupt);}
    let word=|start:usize|u64::from_be_bytes(bytes[start..start+8].try_into().unwrap());
    let h=Head {domain:bytes[..16].try_into().unwrap(),
        volume:[bytes[16..32].try_into().unwrap(),bytes[32..48].try_into().unwrap()],
        recovery:bytes[48..64].try_into().unwrap(),
        objects:[word(64),word(72),word(80)],generation:word(88),
        request:bytes[96..112].try_into().unwrap(),digest:bytes[112..].try_into().unwrap()};
    if h.domain==[0;16] || h.generation==0 || h.request==[0;16] || h.digest==[0;32] ||
       h.objects.contains(&0) || h.objects[0]==h.objects[1] ||
       h.objects[0]==h.objects[2] || h.objects[1]==h.objects[2] {
        return Err(Fault::Corrupt);
    }
    Ok(h)
}

/// Retrait explicitement demandé par le propriétaire ; aucun autre dossier,
/// facteur, groupe ou droit indépendant n'est modifié.
impl<D:Device,A:Authority> Registry<'_,D,A> {
    pub fn revoke_folder(&mut self,grant:Grant,object:u64,expected:Hash,request:Id)->Result<Head> {
        let (h,mut s)=self.view()?;
        if h!=grant.head {return Err(Fault::Conflict);}
        let owner=s.users.iter_mut().find(|u|u.id==grant.owner && u.status==Status::Active)
            .ok_or(Fault::Denied)?;
        let at=owner.folders.iter().position(|f|*f==(object,expected)).ok_or(Fault::Denied)?;
        owner.folders.remove(at);
        self.publish_state(h,s,request,h.generation.checked_add(1).ok_or(Fault::Limit)?)
    }
}
