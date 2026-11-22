"""R2 uniquement dans un export QEMU explicite, après le raccord R1.

L'époque vient du vérificateur sur sa route privée ; ni argv ni NovaFS ne la
choisissent. Le champ epoch de Binding reste la révision d'installation.
"""
from pathlib import Path


def install(root):
    root = Path(root)

    def edit(name, before, after):
        p = root / name
        data = p.read_text()
        if data.count(before) != 1:
            raise ValueError(f"ancrage R2 absent ou ambigu : {name}: {before[:80]}")
        p.write_text(data.replace(before, after))

    base = 'novaOS/kernel/src/sched/process/ipc/'
    path = base + 'exec.rs'
    edit(path, '    remote: Option<authority::Remote>,',
         '    remote: Option<authority::Remote>,\n    authentication_epoch:u64,')
    edit(path, '        administrative:None,', '        administrative:None,\n        authentication_epoch:0,')
    path = base + 'exec_snapshot_lab.rs'
    edit(path, '    administrative:Option<administration::Permit>,',
         '    administrative:Option<administration::Permit>,\n    authentication_epoch:Option<u64>,')
    edit(path, '            administrative:None,',
         '            administrative:None,\n            authentication_epoch:None,')
    edit(path, '        b"NXLDADM1" if p == r.issuer => {', '''        b"NXLDUSR1" if p == r.issuer => {
            let binding=a.binding.ok_or(DENIED)?;
            let epoch=word(q,104);
            if a.authentication_epoch.is_some() || a.sealed.is_none() || epoch==0 || epoch>=1<<63 ||
                q[24..56]!=a.head || q[56..88]!=lease.id || q[88..104]!=binding.holder || q[112..]!=[0;16] {
                return Err(DENIED);
            }
            a.authentication_epoch=Some(epoch);
        }
        b"NXLDADM1" if p == r.issuer => {''')
    edit(path, 'scope==0 || scope&!15!=0 || epoch==0',
         'scope==0 || scope&!15!=0 || epoch==0 || a.authentication_epoch!=Some(epoch)')
    edit(path, '            let binding = a.binding.ok_or(DENIED)?;\n            let sealed',
         '            let binding = a.binding.ok_or(DENIED)?;\n            let authentication_epoch=a.authentication_epoch.ok_or(DENIED)?;\n            let sealed')
    edit(path, '                    a.administrative,\n',
         '                    a.administrative,\n                    authentication_epoch,\n')
    edit(path, 'a.scope, a.head, a.administrative,',
         'a.scope, a.head, a.administrative, authentication_epoch,')
    path = base + 'exec_delivery_lab.rs'
    edit(path, '    administrative:Option<super::session_relay::administration::Permit>,',
         '    administrative:Option<super::session_relay::administration::Permit>,\n    authentication_epoch:u64,')
    edit(path, '    let permit = Permit {',
         '    if authentication_epoch==0 || authentication_epoch>=1<<63 {return Err(DENIED);}\n    let permit = Permit {')
    edit(path, '    s.processes.get_mut(owner).exec[0].relayed = Some(false);',
         '    s.processes.get_mut(owner).exec[0].authentication_epoch=authentication_epoch;\n    s.processes.get_mut(owner).exec[0].relayed = Some(false);')
    path = base + 'session_pipeline_lab.rs'
    edit(path, '    administrative:Option<administration::Permit>,\n}',
         '    administrative:Option<administration::Permit>,\n    authentication_epoch:u64,\n}')
    edit(path, '    administrative:Option<administration::Permit>,\n) -> Res<u64> {',
         '    administrative:Option<administration::Permit>,\n    authentication_epoch:u64,\n) -> Res<u64> {')
    edit(path, '        administrative,\n    });',
         '        administrative,\n        authentication_epoch,\n    });')
    edit(path, '        v.administrative,\n    )',
         '        v.administrative,\n        v.authentication_epoch,\n    )')
    path = base + 'exec_authority_lab.rs'
    edit(path, 'let mut words=[0u64;29];words[28]=r.generation;',
         '''let mut words=[0u64;32];words[28]=r.generation;
    for i in 0..2 {words[29+i]=u64::from_le_bytes(t.binding.holder[8*i..8*i+8].try_into().unwrap());}
    words[31]=t.authentication_epoch;''')
    path = 'novaOS/user/novaauth/service/src/bin/common/administration_channel.rs'
    edit(path, '    let scope = number(&a[8..]);\n    if scope == 0 {', '''    let scope = number(&a[8..]);
    if number(&a[16..])!=current.epoch || current.epoch==0 {return Err(rt::REVOKED);}
    let mut user=[0;128];user[..8].copy_from_slice(b"NXLDUSR1");
    user[8..24].copy_from_slice(&nonce);user[24..56].copy_from_slice(&current.head);
    user[56..88].copy_from_slice(&sid);user[88..104].copy_from_slice(&current.subject);
    user[104..112].copy_from_slice(&current.epoch.to_le_bytes());
    rt::send_for(rt::config(5),1,&user,250)?;
    if scope == 0 {''')
    edit('novaOS/kernel/build.rs', '"--features","administration-guest","--bins"',
         '"--features","administration-r2-guest","--bins"')

    # Autorisation MOA/MOE : conversations duplex privées, export R2 seulement.
    import runpy
    runpy.run_path(str(root / "novaOS/user/novaauth/service/administration_sessions_profile.py"))["duplex"](root)
    runpy.run_path(str(root / "novaOS/user/novaauth/service/administration_sessions_profile.py"))["output_observer"](root)

    routes = runpy.run_path(str(root / "novaOS/user/novaauth/service/administration_routes_profile.py"))
    for phase in ("install", "services", "verifier", "scoped_cleanup", "shell_probe", "shared_sessions", "startup_words", "isolate_closed_route", "route_observer", "output_endpoint_closed"):
        routes[phase](root)
