"""Collecte bornée des connexions dans le seul export EXEC QEMU.

Ne modifie ni les échéances, ni les décisions, ni les preuves noyau/VFS.
"""
from pathlib import Path

def install(root):
    p=Path(root)/'novaOS/user/novaauth/service/src/bin/common/shell_verifier.rs'
    s=p.read_text()
    def edit(old,new):
        nonlocal s
        if s.count(old)!=1:raise ValueError('ancrage auth trace absent/ambigu: '+old[:60])
        s=s.replace(old,new)
    first,rest=s.split('\n',1)
    s=first+'\n'+'''#[derive(Default)]
struct AuthTrace { token:[u8;32], issued:u64, until:u64, signed:u64, verified:u64 }
impl AuthTrace {
    fn emit(&self,tag:u64,code:u64,bail:u64,fin:u64) {
        // 7 + 64 + 1 + 8*17 = 208 octets, sans allocation ni secret.
        let mut out=[b' ';208];out[..7].copy_from_slice(b"NXAUTH ");
        let hex=b"0123456789abcdef";
        for (i,b) in self.token.iter().enumerate(){out[7+i*2]=hex[(b>>4) as usize];out[8+i*2]=hex[(b&15) as usize];}
        let mut pos=72;
        for n in [tag,self.issued,self.until,self.signed,self.verified,code,bail,fin] {
            for shift in (0..16).rev(){out[pos]=hex[((n>>(shift*4))&15) as usize];pos+=1;}
            pos+=1;
        }
        let _=rt::call(rt::DEBUG_WRITE,out.as_ptr() as u64,out.len() as u64,0);
    }
}
'''+rest
    edit('struct Login {','struct Login {\n    auth_trace:AuthTrace,')
    edit('        Self {','        Self {\n            auth_trace:AuthTrace::default(),')
    edit('        let signed = self.signer(b"NSHSIG01", digest)?;', '''        self.auth_trace=AuthTrace{token:digest,issued:issued_at,until:issued_at+u64::from(TTL_MS),..AuthTrace::default()};
        let signed = self.signer(b"NSHSIG01", digest)?;
        self.auth_trace.signed=now()?;''')
    edit('        let received_at = now()?;','        let received_at = now()?;\n        self.auth_trace.verified=received_at;')
    start=s.index('                marker(\n                    if received_at')
    end=s.index('                rt::DENIED',start)
    # La raison et la chronologie du refus sont dans le même NXAUTH que son
    # résultat. Éviter deux DEBUG par connexion ; aucune erreur n'est cachée.
    s=s[:start]+s[end:]
    edit('        login.checked = None;','        login.checked = None;\n        login.auth_trace=AuthTrace::default();')
    edit('        let op = &m.payload[..8];','''        let op = &m.payload[..8];
        if op==b"NSHOPEN1" {login.auth_trace.emit(m.tag,result.err().unwrap_or(0),login.relay_deadline,now().unwrap_or(0));}''')
    old='''        if matches!(op, b"NSHOPEN1" | b"NSHEXEND" | b"NSHEXRD1" | b"NXPKBEG1" | b"NXPKEND1" | b"NSHCLOS1")
            || (result.is_err() && matches!(op, b"NSHEXBG1" | b"NSHLIVE1")) {'''
    edit(old,'''        // Admissions, révocations, PID et VFS restent tracés par leurs
        // effecteurs. Les succès de relais redondants ne consomment plus le quota.
        if op!=b"NSHOPEN1" && result.is_err() {''')
    p.write_text(s)
