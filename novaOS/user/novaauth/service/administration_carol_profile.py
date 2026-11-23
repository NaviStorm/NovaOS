"""Facteur Carol après création persistante, seconde route privée QEMU existante.

La clé du signataire 25 est fournie séparément au build privé. Le registre ne
reçoit que la clé publique préprovisionnée. Aucun paquet ni droit hérité.
"""
from pathlib import Path

def install(root):
    root=Path(root)
    p=root/'components/shell/session_auth_lab.rs';s=p.read_text()
    old='        if owner[8..24] == [0; 16] {\n            return Err(rt::DENIED);\n        }\n        Ok(())'
    new='        if owner[8..24] == [0; 16] { return Err(rt::DENIED); }\n        let mut message=[0u8;52];message[..20].copy_from_slice(b"R2IDENTITY subject= ");\n        let digits=b"0123456789abcdef";\n        for (i,b) in owner[8..24].iter().enumerate() {message[20+i*2]=digits[(b>>4) as usize];message[21+i*2]=digits[(b&15) as usize];}\n        rt::value(rt::call(rt::DEBUG_WRITE,message.as_ptr() as u64,message.len() as u64,0))?;\n        Ok(())'
    if s.count(old)!=1:raise ValueError('ancrage identité authentifiée')
    p.write_text(s.replace(old,new))
    p=root/'novaOS/user/novaauth/service/administration_bob_shell.rs';s=p.read_text()
    old='    login.close()?;'
    new='    for name in ["whoami", "admin-sonde", "alice-seulement"] {\n        if !matches!(login.execute(name,&[]),Err(rt::DENIED)) {return Err(rt::REFUSED);}\n    }\n    mark(b"NAR2 Carol sans installations heritees statut=5 aucun enfant")?;\n    login.close()?;'
    if s.count(old)!=1:raise ValueError('ancrage fermeture seconde route')
    p.write_text(s.replace(old,new))
