"""Sondes bornées du seul export ticket-vivant. Aucune clé ni octet privé."""
from pathlib import Path

def install(root):
    root = Path(root)
    def edit(name, old, new):
        p=root/name
        s=p.read_text()
        if s.count(old)!=1:
            raise ValueError('ancrage trace absent/ambigu : '+name+' : '+old[:60])
        p.write_text(s.replace(old,new))

    bob='novaOS/user/novaauth/service/administration_bob_shell.rs'
    helper = '''
fn probe(stage:u64, iteration:u64, kind:u64, native:u64) {
    let text=alloc::format!("R2PROBE s={} i={} k={} n={}",stage,iteration,kind,native);
    let _=mark(text.as_bytes());
}
'''
    edit(bob,'pub(super) fn session()',helper+'\npub(super) fn session()')
    edit(bob,'for _ in 0..300 {','for iteration in 0..300 {')
    edit(bob,'            _=>return Err(rt::REFUSED),\n        }\n        match rt::receive_for', '''            Err(code)=>{probe(1,iteration,1,code);return Err(rt::REFUSED);},
            Ok(_)=>{probe(1,iteration,0,0);return Err(rt::REFUSED);},
        }
        match rt::receive_for''')
    edit(bob,'''            _=>return Err(rt::REFUSED),
        }
    }
    if !retired {return Err(rt::REFUSED);}''','''            Err(code)=>{probe(2,iteration,1,code);return Err(rt::REFUSED);},
            Ok(m)=>{probe(2,iteration,0,m.kind as u64);return Err(rt::REFUSED);},
        }
    }
    if !retired {probe(3,300,2,0);return Err(rt::REFUSED);}''')
    edit(bob,'if answer.status!=rt::REVOKED {return Err(rt::REFUSED);}',
         'if answer.status!=rt::REVOKED {probe(4,0,1,answer.status);return Err(rt::REFUSED);}')
    edit(bob,'    // Observer au plus 6 s ;', '    // 300 attentes de 5 ticks, hors consultations et ordonnancement ;')

    shell='components/shell/session_auth_lab.rs'
    edit(shell,'    pub(super) fn receipt(key: u64)', '''    fn probe(stage:u64, native:u64) {
        let text=alloc::format!("R2PROBE s={} n={}",stage,native);
        let _=rt::call(rt::DEBUG_WRITE,text.as_ptr() as u64,text.len() as u64,0);
    }
    pub(super) fn receipt(key: u64)''')
    edit(shell,'        rt::value(rt::call(rt::IPC_SEND, key, (&raw const q) as u64, 160))?;',
         '        rt::value(rt::call(rt::IPC_SEND, key, (&raw const q) as u64, 160)).map_err(|e|{Self::probe(11,e);e})?;')
    edit(shell,'        let a = rt::receive_for(key, 250)?;',
         '        let a = rt::receive_for(key, 250).map_err(|e|{Self::probe(12,e);e})?;')
    edit(shell,'''        if a.tag != 1 || a.kind != 1 || a.len != 32 || &a.payload[..8] != b"NXREPLY1" {
            return Err(rt::REFUSED);''','''        if a.tag != 1 || a.kind != 1 || a.len != 32 || &a.payload[..8] != b"NXREPLY1" {
            Self::probe(13,rt::REFUSED);
            return Err(rt::REFUSED);''')
    edit(shell,'''        let code = u64::from_le_bytes(a.payload[8..16].try_into().unwrap());
        if code != 0 {''','''        let code = u64::from_le_bytes(a.payload[8..16].try_into().unwrap());
        if !matches!(code,rt::OK|rt::WOULD_BLOCK|rt::REVOKED) {Self::probe(14,code);}
        if code != 0 {''')

    syscall='novaOS/kernel/src/sched/process/syscall.rs'
    edit(syscall,'        hal::log(format_args!("DEBUG pid={pid} {msg}"));',
         '''        if msg.starts_with("R2PROBE ") {
            hal::log(format_args!("R2TIME pid={pid} ticks={} {msg}",ticks()));
        } else {
            hal::log(format_args!("DEBUG pid={pid} {msg}"));
        }''')

    snap='novaOS/kernel/src/sched/process/ipc/exec_snapshot_lab.rs'
    for command in ['NXLDSIZE','NXLDSEAL']:
        anchor='        b"'+command+'" if p == r.storage => {'
        edit(snap,anchor,anchor+'\n            hal::log(format_args!("R2LOAD owner={} step='+command+' ticks={}",r.shell.number,ticks()));')
    anchor='            a.sealed = Some(a.staging.take().ok_or(DENIED)?.seal().map_err(geometry)?);'
    edit(snap,anchor,anchor+'\n            hal::log(format_args!("R2LOAD owner={} step=SEALED ticks={}",r.shell.number,ticks()));')

    authority='novaOS/kernel/src/sched/process/ipc/exec_authority_lab.rs'
    edit(authority,'hal::log(format_args!("NAEXEC enfant owner={} child={}",owner.number,child.number));',
         'hal::log(format_args!("NAEXEC enfant owner={} child={} ticks={}",owner.number,child.number,ticks()));')
    edit(authority,'hal::log(format_args!("NAEXEC transition registry={} owner={} kind={} value={value}",p.number,owner.number,data[8]));',
         'hal::log(format_args!("NAEXEC transition registry={} owner={} kind={} value={value} ticks={}",p.number,owner.number,data[8],ticks()));')
    channel='novaOS/user/novaauth/service/src/bin/common/administration_channel.rs'
    start='                if matches!(admin_command::decode(request),Ok(admin_command::Command::Confirm{..})) {'
    edit(channel,start,start+'\n                    r2_probe(20);')
    anchor='                answer = exchange(login, request.try_into().unwrap())?;'
    edit(channel,anchor,'''                let trace_confirm=matches!(admin_command::decode(request),Ok(admin_command::Command::Confirm{..}));
                if trace_confirm {r2_probe(21);}
'''+anchor+'''
                if trace_confirm {r2_probe(22);}
''')
    p=root/channel
    p.write_text(p.read_text()+'''
fn r2_probe(stage:u64) {
    let text:&[u8]=match stage {20=>b"R2PROBE s=20 n=0",21=>b"R2PROBE s=21 n=0",_=>b"R2PROBE s=22 n=0"};
    let _=rt::call(rt::DEBUG_WRITE,text.as_ptr() as u64,text.len() as u64,0);
}
''')
