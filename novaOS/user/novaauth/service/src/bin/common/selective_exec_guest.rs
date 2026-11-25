//! Descripteur du ticket EXEC. L'émission actuelle reste limitée au lancement
//! sans portée VFS ; une portée liée exige aussi un sceau vérifié sur P9.
use super::*;
use novaos_identity::{
 recuperation_view::View, policy::{Image,Origin},
 distribution::{ExecutionIdentity,Machine}, execution_selective::RequiredPermission,
};
use novaos_recovery::exec_descriptor::{Descriptor,DEPENDENCY,HEADER as DESCRIPTOR_HEADER};
const ALL:u64=0x4e5847415445414c;
fn machine()->Machine {if cfg!(target_arch="aarch64"){Machine::Aarch64}else{Machine::X86_64}}
fn distribution()->rt::Result<VerifyingKey>{novaos_identity::distribution::public_key(include_bytes!(env!("NOVA_PACKAGE_PUBLIC"))).map_err(|_|rt::REFUSED)}
fn number(p:&[u8],at:usize)->u64{u64::from_be_bytes(p[at..at+8].try_into().unwrap())}
fn put(p:&mut [u8],at:usize,n:u64){p[at..at+8].copy_from_slice(&n.to_be_bytes());}
fn query(op:[u8;16],answer:&mut [u8;MAX])->rt::Result<(usize,[u8;112])>{
 let mut q=[0;112];q[..16].copy_from_slice(&incarnation::SERVER);q[16..32].copy_from_slice(&incarnation::DOMAIN);
 q[32..64].copy_from_slice(&incarnation::identity());q[64..80].copy_from_slice(&op);q[80..].copy_from_slice(&*common::random()?);
 let n=exchange(63,&q,answer)?;novaos_recovery::exec_descriptor::verify_lookup(&key()?,&q,&answer[HEADER+8..HEADER+8+n]).map_err(|_|rt::REFUSED)?;Ok((n,q))
}
/// La précision de portée est produite par R à partir de l'installation
/// attestée et du dossier de sa vue, sans accepter d'origine du demandeur.
pub(super) fn resolve_file(state: &View, storage: &mut Storage, grant: novaos_identity::Grant,
    descriptor: &mut [u8; 240], with_file: bool) -> rt::Result<()> {
    let image = Image {
        installation: descriptor[56..72].try_into().unwrap(),
        digest: descriptor[152..184].try_into().unwrap(),
    };
    let Some(required) = state.required_exec_read(grant, image).map_err(|e| {
        if matches!(e, novaos_identity::Fault::Conflict) { 24 } else { rt::REVOKED }
    })? else {
        return Ok(());
    };
    if state.execution_identity(grant, image, &distribution()?, machine())
        .map_err(|_| rt::REVOKED)?.administration != 0 {
        return Err(rt::DENIED);
    }
    descriptor[..8].copy_from_slice(b"NXRES002");
    descriptor[184..232].fill(0);
    if with_file {
        let operation = common::random()?[..16].try_into().unwrap();
        let scope = storage.select_exec_file(operation, state.head().volume,
            required.owner, required.object, required.seal)?;
        descriptor[184..192].copy_from_slice(&scope.directory.to_le_bytes());
        descriptor[192..200].copy_from_slice(&scope.file.to_le_bytes());
        descriptor[200..208].copy_from_slice(&scope.directory.to_le_bytes());
    }
    Ok(())
}

pub(super) fn validate(state:&View,storage:&mut Storage,d:Descriptor<'_>,m:&[u64;WORDS])->rt::Result<()> {
 let p=d.bytes();
 let file=d.matches_launch_metadata(m).map_err(|_|rt::REVOKED)?;
 if p[8..24]!=incarnation::SERVER || p[24..40]!=incarnation::DOMAIN || p[40..72]!=incarnation::identity()
  || p[328..344]!=state.head().volume[0] || p[344..360]!=state.head().volume[1] {return Err(rt::REVOKED);}
 let own=d.dependencies().next().ok_or(rt::REFUSED)?;
 let expected=ExecutionIdentity{package:p[272..288].try_into().unwrap(),version:u32::from_be_bytes(p[288..292].try_into().unwrap()),machine:u16::from_be_bytes(p[292..294].try_into().unwrap()),manifest:p[392..424].try_into().unwrap(),object:number(p,360),revision:number(p,368),offset:number(p,376),size:number(p,384),image_revision:own.image_revision(),administration:p[504]};
 let required=if let Some(dep)=file {
  let seal=storage.exec_seal(d.operation(),state.head().volume,dep.owner(),dep.object(),m[33])?;
  Some(RequiredPermission{
   owner:dep.owner(),object:dep.object(),rights:dep.rights(),
   origin:if dep.origin()==1{Origin::Direct}else{Origin::Group(dep.group())},
   membership_revision:dep.membership_revision(),rule_revision:dep.rule_revision(),
   image_revision:dep.image_revision(),seal,
  })
 }else{None};
 state.revalidate_execution_subject(p[136..152].try_into().unwrap(),m[31],Image{installation:bytes(&m[12..14]),digest:bytes(&m[14..18])},expected,&distribution()?,machine(),required.as_slice()).map_err(|_|rt::REVOKED)
}
pub(super) fn digest(state:&View,storage:&mut Storage,m:&[u64;WORDS])->rt::Result<[u8;32]>{
 let op=bytes::<16>(&m[26..28]);let mut a=[0;MAX];let(n,q)=query(op,&mut a)?;
 let(_,d)=novaos_recovery::exec_descriptor::verify_lookup(&key()?,&q,&a[HEADER+8..HEADER+8+n]).map_err(|_|rt::REFUSED)?;
 validate(state,storage,d,m)?;Ok(d.digest())
}
pub(super) fn register(state:&View,storage:&mut Storage,q:&[u8;128])->rt::Result<()> {
 if &q[..8]!=b"NXEXBND1" || q[96..]!=[0;32]{return Err(rt::BAD_ARGUMENT);}
 let mut cursor=0;let mut found=None;
 // Au plus deux routes nsh. Ne jamais choisir une instance sur son seul PID.
 for _ in 0..2 {
  let call=rt::call(rt::IPC_NOTIFY,rt::config(14),ALL,cursor);
  if call.status!=0{return Err(call.status);}let leaf=call.value;if leaf==0{break;}cursor=leaf;
  let m=metadata(leaf)?;
  if m[1]==common::number(&q[88..]) && bytes::<32>(&m[4..8])==q[24..56] && bytes::<32>(&m[8..12])==q[56..88] {found=Some((leaf,m));break;}
 }
 let(leaf,m)=found.ok_or(rt::DENIED)?;
 // Le chargeur isolé ne publie encore aucune portée VFS attestée. Un
 // lancement cadré ne doit pas retomber sur le chemin historique sans origine.
 if m[32..35]!=[0;3]{return Err(rt::REVOKED);}
 if m[35]!=1{return Ok(());}
 if m[0]!=0 || m[26..28]!=[0;2]{return Err(rt::DENIED);}
 let fresh=tete::query(rt::config(8),&incarnation::identity())?;
 if fresh[72..80]!=state.head().generation.to_be_bytes() || fresh[80..]!=state.head().digest || fresh[80..]!=bytes::<32>(&m[18..22]) {return Err(rt::REVOKED);}
 let grant=state.resolve_verified_factor(q[8..24].try_into().unwrap()).map_err(|_|rt::DENIED)?;
 if grant.subject()!=bytes::<16>(&m[29..31]) || state.authentication_epoch(grant).map_err(|_|rt::REVOKED)?!=m[31]{return Err(rt::REVOKED);}
 let own=state.execution_identity(grant,Image{installation:bytes(&m[12..14]),digest:bytes(&m[14..18])},&distribution()?,machine()).map_err(|_|rt::DENIED)?;
 let mut p=[0;DESCRIPTOR_HEADER+DEPENDENCY];p[..8].copy_from_slice(b"NXEXDP01");p[8..24].copy_from_slice(&incarnation::SERVER);p[24..40].copy_from_slice(&incarnation::DOMAIN);p[40..72].copy_from_slice(&incarnation::identity());
 p[72..104].copy_from_slice(&q[24..56]);p[104..136].copy_from_slice(&bytes::<32>(&m[43..47]));p[136..152].copy_from_slice(&grant.subject());put(&mut p,152,m[31]);
 p[160..192].copy_from_slice(&q[56..88]);p[192..224].copy_from_slice(&q[24..56]);let op:[u8;16]=common::random()?[..16].try_into().unwrap();p[224..240].copy_from_slice(&op);
 p[240..256].copy_from_slice(&bytes::<16>(&m[12..14]));p[256..272].copy_from_slice(&grant.subject());p[272..288].copy_from_slice(&own.package);p[288..292].copy_from_slice(&own.version.to_be_bytes());p[292..294].copy_from_slice(&own.machine.to_be_bytes());p[294]=1;p[295]=1;
 p[296..328].copy_from_slice(&bytes::<32>(&m[14..18]));p[328..344].copy_from_slice(&state.head().volume[0]);p[344..360].copy_from_slice(&state.head().volume[1]);
 for(at,n)in[(360,own.object),(368,own.revision),(376,own.offset),(384,own.size),(424,state.head().generation),(464,m[3])]{put(&mut p,at,n);}
 p[392..424].copy_from_slice(&own.manifest);p[432..464].copy_from_slice(&state.head().digest);p[472..504].copy_from_slice(&q[24..56]);p[504]=own.administration;
 p[512]=1;p[513]=1;p[520..536].copy_from_slice(&grant.subject());put(&mut p,536,own.object);put(&mut p,576,own.image_revision);
 let d=Descriptor::parse(&p).map_err(|_|rt::REFUSED)?;validate(state,storage,d,&m)?;
 let mut answer=[0;MAX];match exchange(62,&p,&mut answer) {
  Ok(n)=>{let a=&answer[HEADER+8..HEADER+8+n];if n!=p.len()+72 || a[8..8+p.len()]!=p{return Err(rt::REFUSED);}
   d.verify_registration(&key()?,number(a,0),a[8+p.len()..].try_into().unwrap()).map_err(|_|rt::REFUSED)?;}
  Err(UNCERTAIN)=>{let(n,q)=query(op,&mut answer)?;let(_,found)=novaos_recovery::exec_descriptor::verify_lookup(&key()?,&q,&answer[HEADER+8..HEADER+8+n]).map_err(|_|rt::REFUSED)?;if found.bytes()!=p{return Err(rt::REFUSED);}}
  Err(e)=>return Err(e),
 }
 command(leaf,4,op,0,[0;32])?;common::marker(b"NXSELECT descripteur Direct durable lie avant remise");Ok(())
}

/// Réconciliation d'une annulation déjà établie par le noyau. Aucun contrôle de
/// politique ne peut transformer cette consultation en nouvelle admission.
pub(super) fn cancelled_digest(m:&[u64;WORDS],request:&Request)->rt::Result<[u8;32]>{
 let mut a=[0;MAX];let(n,q)=query(bytes::<16>(&m[26..28]),&mut a)?;
 let(_,d)=novaos_recovery::exec_descriptor::verify_lookup(&key()?,&q,&a[HEADER+8..HEADER+8+n]).map_err(|_|rt::REFUSED)?;
 d.matches(request).map_err(|_|rt::REFUSED)?;Ok(d.digest())
}
