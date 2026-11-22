"""Deux routes privées R2. Ne s'applique jamais aux sources du noyau ordinaire."""
from pathlib import Path


def install(root):
    root = Path(root)
    base = 'novaOS/kernel/src/sched/process/'
    ipc = base + 'ipc/'

    def edit(name, old, new, count=1):
        p = root / name
        s = p.read_text()
        if s.count(old) != count:
            raise ValueError(f'ancrage routes R2 absent ou ambigu : {name}: {old[:100]}')
        p.write_text(s.replace(old, new))

    edit(base+'mod.rs' if (root/(base+'mod.rs')).exists() else 'novaOS/kernel/src/sched/process.rs',
         '    relay: Option<ipc::session_relay::Route>,',
         '    relays: [Option<ipc::session_relay::Route>; 2],')
    edit('novaOS/kernel/src/sched/process.rs', '            relay: None,',
         '            relays: [None, None],')
    path = ipc+'session_relay.rs'
    edit(path, '    if s.processes.relay.as_ref().is_some_and(|r| !r.down)',
         '''    // Deux instances désignées au boot, aucune réutilisation d'un slot
    // tant que son ancienne route et ses ressources sont encore présentes.
    let at = s.processes.relays.iter().position(Option::is_none).ok_or(LIMIT)?;
    if s.processes.relays.iter().flatten().any(|r| r.shell == shell || r.relay == relay)''')
    edit(path, '    s.processes.relay = Some(Route {', '    s.processes.relays[at] = Some(Route {')
    edit(path, '    if live(s, r.storage).is_ok() {\n        service::config(s, r.storage, 72, &[0]);',
         '    if live(s, r.storage).is_ok() && r.console_route {\n        service::config(s, r.storage, 72, &[0]);')
    edit(path, '    shell_instance: Instance,', '    console_route: bool,\n    shell_instance: Instance,')
    edit(path, '        shell_instance,\n        auth_port,',
         '        console_route: live(s,shell)?.terminal & (crate::terminal::OUTPUT | crate::terminal::ERROR) == (crate::terminal::OUTPUT | crate::terminal::ERROR),\n        shell_instance,\n        auth_port,')
    edit(path, '            service::config(s, r.storage, 73, &[word(q, 80), word(q, 88)]);\n            service::config(s, r.storage, 72, &[seq]);',
         '''            if r.console_route {
                service::config(s, r.storage, 73, &[word(q, 80), word(q, 88)]);
                service::config(s, r.storage, 72, &[seq]);
            }''')
    p = root/path
    s = p.read_text()
    start=s.index('pub(in crate::sched::process) fn retired(')
    end=s.index('\nfn validate_recipient', start)
    section=s[start:end]
    section=section.replace('    if let Some(mut r) = s.processes.relay.take() {',
                            '    for at in 0..2 { if let Some(mut r) = s.processes.relays[at].take() {')
    section=section.replace('s.processes.relay = Some(r);','s.processes.relays[at] = Some(r);')
    section=section.replace('\n}\n\npub(super) fn expire', '\n    }\n}\n\npub(super) fn expire')
    section=section.replace('    let Some(mut r) = s.processes.relay.take() else {\n        return;\n    };',
                            '    for at in 0..2 {\n    let Some(mut r) = s.processes.relays[at].take() else { continue; };')
    section=section.rstrip()[:-1]+'    }\n}\n'
    s=s[:start]+section+s[end:]
    start=s.index('pub(super) fn intercept(')
    end=s.index('\npub(super) fn owns_session',start)
    section=s[start:end].replace('fn intercept(s: &mut Core, p:', 'fn intercept_route(s: &mut Core, at: usize, p:')
    section=section.replace('s.processes.relay','s.processes.relays[at]')
    s=s[:start]+section+s[end:]
    start=s.index('pub(super) fn owns_session')
    s=s[:start]+'''pub(super) fn owns_session(s: &Core, shell: Pid, id: [u8; 32]) -> bool {
    s.processes.relays.iter().flatten().any(|r| {
        r.shell == shell && r.live.is_some_and(|l| l.id == id && time_ms() < l.limit)
    })
}
pub(in crate::sched::process) fn timer(s: &mut Core) {
    for at in 0..2 {
        if let Some(mut r) = s.processes.relays[at].take() {
            selection::tick(s, &mut r);
            s.processes.relays[at] = Some(r);
        }
    }
    expire(s);
}
'''
    s+='\n'+(root/'novaOS/user/novaauth/service/administration_routes.rs').read_text()
    p.write_text(s)
    # Les points d'attachement sélectionnent le propriétaire attesté.
    path=ipc+'session_display_lab.rs'
    edit(path, '    if !owns_session(s, shell, binding.session) || live(s, shell)?.terminal & mask != mask {',
         '''    let at = route_for(s, shell)?;
    let selected = s.processes.relays[at].as_ref().ok_or(REVOKED)?;
    if !owns_session(s, shell, binding.session)
        || (selected.console_route && live(s, shell)?.terminal & mask != mask)
        || (!selected.console_route && (scope != [0;3] || stdin)) {''')
    edit(path, '    let r = s.processes.relay.as_ref().ok_or(REVOKED)?;',
         '    let r = s.processes.relays[at].as_ref().ok_or(REVOKED)?;')
    edit(path, '    let storage = r_storage(s)?;\n    service::config(s, storage, 96 + available * 16, &[0]);',
         '''    let storage = s.processes.relays[at].as_ref().ok_or(REVOKED)?.storage;
    if s.processes.relays[at].as_ref().ok_or(REVOKED)?.console_route {
        service::config(s, storage, 96 + available * 16, &[0]);
    }''')
    edit(path, '    let r=s.processes.relay.as_ref().ok_or(REVOKED)?;',
         '    let at=route_for(s,shell)?;\n    let r=s.processes.relays[at].as_ref().ok_or(REVOKED)?;')
    edit(path, 's.processes.relay.as_mut().unwrap()', 's.processes.relays[at].as_mut().unwrap()', 2)
    edit(path, '''fn r_storage(s: &Core) -> Res<Pid> {
    s.processes.relay.as_ref().map(|r| r.storage).ok_or(REVOKED)
}''', '''impl Capture { pub(super) fn belongs(&self, p:Pid)->bool {self.child==p} }''')
    edit(path, '    let capture = s.processes.relay.as_ref().and_then(|r| {',
         '    let capture = route_for(s,owner).ok().and_then(|at|s.processes.relays[at].as_ref()).and_then(|r| {')
    path=ipc+'session_admin_lab.rs'
    edit(path, '    let r = s.processes.relay.as_ref().ok_or(REVOKED)?;',
         '    let at_route=route_for(s,owner)?;\n    let r = s.processes.relays[at_route].as_ref().ok_or(REVOKED)?;')
    edit(path, 's.processes.relay.as_mut().ok_or(REVOKED)?', 's.processes.relays[at_route].as_mut().ok_or(REVOKED)?')
    with (root/path).open('a') as f:
        f.write('''\nimpl Channel {pub(super) fn belongs(&self,p:Pid,issuer:Pid,key:u64)->bool {
    (p==self.child && key==self.leaf) || (p==issuer && key==self.peer)
}}\n''')
    path=ipc+'session_pipeline_lab.rs'
    edit(path, '    let v = s\n        .processes\n        .relay\n        .as_ref()',
         '    let at=route_for(s,owner)?;\n    let v = s.processes.relays[at].as_ref()')
    edit(path, 's.processes.relay.as_mut().unwrap()', 's.processes.relays[at].as_mut().unwrap()')
    path=ipc+'selection_vfs_lab.rs'
    edit(path, 'fn apply<T>(f:', 'fn apply<T>(parent: Pid, f:')
    edit(path, '        let mut r = s.processes.relay.take().ok_or(REVOKED)?;',
         '        let at=route_for(s,parent)?;\n        let mut r = s.processes.relays[at].take().ok_or(REVOKED)?;')
    edit(path, '        s.processes.relay = Some(r);', '        s.processes.relays[at] = Some(r);')
    edit(path, '        let r = s.processes.relay.as_ref()?;',
         '        let r = s.processes.relays[route_for(s,p).ok()?].as_ref()?;')
    p=root/path;s=p.read_text();s=s.replace('apply(|','apply(parent, |');p.write_text(s)
    with p.open('a') as f:
        f.write('\nimpl Bridge {pub(super) fn matches_sequence(&self,key:u64)->bool {key!=0 && self.sequence==key}}\n')
    with (root/(ipc+'exec_snapshot_lab.rs')).open('a') as f:
        f.write('\nimpl Admission {pub(super) fn matches_request(&self,id:&[u8])->bool {id==self.request}}\n')


def services(root):
    """Instances fixes désignées par le lanceur ; aucune sélection par l'invité."""
    root=Path(root)
    def edit(name,old,new,count=1):
        p=root/name;s=p.read_text()
        if s.count(old)!=count: raise ValueError(f'ancrage services R2 : {name}: {old[:80]}')
        p.write_text(s.replace(old,new))
    base='novaOS/kernel/src/sched/process/ipc/'
    edit(base+'service/shell_auth_lab.rs',
         'pub(super) fn connect(s: &mut Core, m: Session, signer: Pid) -> Res<()> {',
         'pub(super) fn connect(s: &mut Core, m: Session, signer: Pid, second: bool) -> Res<()> {')
    edit(base+'service/shell_auth_lab.rs', '    let shell = live_identity(s, m.client)?;',
         '''    let client = if second { second_shell(s,m)? } else { m.client };
    let shell = live_identity(s, client)?;
    let offset = if second { 21 } else { 0 };''')
    for old,new in [
        ('auth_lab::conversation(s, m.client, m.client, verifier)?','auth_lab::conversation(s, client, client, verifier)?'),
        ('config(s, verifier, 11, &shell);','config(s, verifier, 11+offset, &shell);'),
        ('config(s, verifier, 16, &[replies, challenge_tx, signature_rx]);','config(s, verifier, 16+offset, &[replies, challenge_tx, signature_rx]);'),
        ('config(s, verifier, 15, &[requests]);','config(s, verifier, 15+offset, &[requests]);'),
        ('config(s, m.client, 17, &[reply]);','config(s, client, 17, &[reply]);'),
        ('config(s, m.client, 16, &[ask]);','config(s, client, 16, &[ask]);'),
        ('        m.client.number, verifier.number, signer.number','        client.number, verifier.number, signer.number'),
        ('pub(super) fn connect_relay(s: &mut Core, m: Session, relay: Pid) -> Res<()> {',
         'pub(super) fn connect_relay(s: &mut Core, m: Session, relay: Pid, second: bool) -> Res<()> {\n    let client=if second {second_shell(s,m)?} else {m.client};\n    let offset=if second {21} else {0};'),
        ('register(s, m.client, issuer, relay, m.fs, send, tx)?;','register(s, client, issuer, relay, m.fs, send, tx)?;'),
        ('config(s, issuer, 20, &[receive]);','config(s, issuer, 20+offset, &[receive]);'),
        ('config(s, issuer, 19, &[send]);','config(s, issuer, 19+offset, &[send]);'),
    ]:edit(base+'service/shell_auth_lab.rs',old,new)
    with (root/(base+'service/shell_auth_lab.rs')).open('a') as f:
        f.write('''\nfn second_shell(s:&Core,m:Session)->Res<Pid> {
    let mut peers=s.processes.slots.iter().flatten().filter(|p|
        p.service_owner==Some(m.manager) && p.service_id==24 && p.outcome.is_none() && p.space.is_some());
    let p=peers.next().ok_or(CLOSED)?.pid;
    if peers.next().is_some() {return Err(DENIED);}
    Ok(p)
}\n''')
    path=base+'service.rs'
    edit(path, '        23 if auth_lab::enabled() && argument == 0 => include_bytes!(concat!(env!("OUT_DIR"), "/signataire-incarnation.elf")),',
         '''        23 if auth_lab::enabled() && argument == 0 => include_bytes!(concat!(env!("OUT_DIR"), "/signataire-incarnation.elf")),
        24 if auth_lab::enabled() && argument == 0 => include_bytes!(concat!(env!("OUT_DIR"), "/shell.elf")),
        25 if auth_lab::enabled() && argument == 0 => include_bytes!(concat!(env!("OUT_DIR"), "/shell-signer-bob.elf")),
        26 if auth_lab::enabled() && argument == 0 => include_bytes!(concat!(env!("OUT_DIR"), "/exec-relay.elf")),''')
    edit(path,'matches!(id, 21..=23)', 'matches!(id, 21..=26)')
    edit(path,'shell_auth_lab::connect(s,m,target)?','shell_auth_lab::connect(s,m,target,false)?')
    edit(path,'shell_auth_lab::connect_relay(s,m,target)?',
         'shell_auth_lab::connect_relay(s,m,target,false)?, 25 => shell_auth_lab::connect(s,m,target,true)?, 26 => shell_auth_lab::connect_relay(s,m,target,true)?')
    # Le même ELF nsh ; aucun bit CONSOLE, VFS général ou gestion des services.
    edit('novaOS/user/novaauth/service/exec_admission_launcher.rs','for id in [18,21] {','for id in [18,21,24,25,26] {')
    path='novaOS/user/novaauth/service/src/bin/shell-signer.rs'
    bob=root/'novaOS/user/novaauth/service/src/bin/shell-signer-bob.rs'
    if bob.exists():raise ValueError('signataire Bob déjà présent')
    bob.write_text((root/path).read_text().replace('env!("NOVA_LAB_ALICE")','env!("NOVA_LAB_BOB")'))
    with (root/'novaOS/user/novaauth/service/Cargo.toml').open('a') as f:
        f.write('''\n[[bin]]
name = "shell-signer-bob"
path = "src/bin/shell-signer-bob.rs"
required-features = ["administration-r2-guest"]
test = false
''')
    edit('novaOS/kernel/build.rs','"auth-verifier","auth-client","shell-signer","exec-relay"',
         '"auth-verifier","auth-client","shell-signer","shell-signer-bob","exec-relay"')


def verifier(root):
    root=Path(root)
    path=root/'novaOS/user/novaauth/service/src/bin/common/shell_verifier.rs'
    s=path.read_text()
    def replace(old,new,count=1):
        nonlocal s
        if s.count(old)!=count:raise ValueError(f'ancrage verificateur R2 : {old[:80]}')
        s=s.replace(old,new)
    replace('struct Login {','struct Login {\n    slot: usize,')
    replace('    fn from_launcher() -> Self {\n        Self {','    fn from_launcher(slot: usize) -> Self {\n        Self {\n            slot,')
    replace('client: identity(11),','client: identity(11 + slot * 21),')
    start=s.index('impl Login {');end=s.index('pub(super) fn serve()')
    portion=s[start:end]
    for n in range(15,21):portion=portion.replace(f'rt::config({n})',f'self.port({n})')
    portion=portion.replace('.add(20)', '.add(20 + self.slot * 21)')
    portion=portion.replace('impl Login {','''impl Login {
    fn port(&self, n:usize)->u64 { rt::config(n + self.slot * 21) }''')
    s=s[:start]+portion+s[end:]
    replace('    while rt::config(15) == 0 {','    while rt::config(15) == 0 || rt::config(36) == 0 {')
    replace('''    let mut login = Login::from_launcher();
    let mut administrative=administrative::Channels::new();
    if login.route.client == [0; 32] || login.route.issuer == [0; 32] {
        return Err(rt::DENIED);
    }
    let mut last = 0;
    loop {
        administrative.poll(&mut login);
        let m = match rt::receive_for(rt::config(15), 2) {''', '''    let mut logins = [Login::from_launcher(0), Login::from_launcher(1)];
    let mut channels = [administrative::Channels::new(), administrative::Channels::new()];
    if logins.iter().any(|l|l.route.client==[0;32] || l.route.issuer==[0;32])
        || logins[0].route.client == logins[1].route.client {return Err(rt::DENIED);}
    let mut last_tags = [0;2];
    let (mut store_seq, mut relay_seq) = (0,0);
    loop { for slot in 0..2 {
        let login = &mut logins[slot];
        let last = &mut last_tags[slot];
        // Les ports vers registre/NovaFS sont partagés, donc leurs séquences
        // sont communes. Les conversations de connexion restent indépendantes.
        login.store_seq=store_seq; login.relay_seq=relay_seq;
        channels[slot].poll(login);
        let m = match rt::receive_for(login.port(15), 2) {''')
    replace('''                let _ = login.maintain();
                continue;''','''                let _ = login.maintain();
                store_seq=login.store_seq; relay_seq=login.relay_seq;
                continue;''')
    replace('m.tag <= last {','m.tag <= *last {')
    replace('            last = m.tag;','            *last = m.tag;')
    # Dans la boucle, les accès aux ports d'une session utilisent son offset.
    start=s.index('pub(super) fn serve()');prefix=s[:start];body=s[start:]
    body=body.replace('let port = rt::config(19);','let port = login.port(19);')
    body=body.replace('rt::send_for(rt::config(16),','rt::send_for(login.port(16),')
    ending='''        }
    }
}
'''
    if not body.endswith(ending):raise ValueError('fin boucle verificateur R2')
    body=body[:-len(ending)]+'''        }
        store_seq=login.store_seq; relay_seq=login.relay_seq;
    }}
}
'''
    path.write_text(prefix+body)
    path=root/'novaOS/user/novaauth/service/src/bin/common/administration_channel.rs'
    s=path.read_text();old='rt::config(5), i as u64, LIST'
    if s.count(old)!=1:raise ValueError('LIST prive R2')
    path.write_text(s.replace(old,'rt::config(5), (login.slot * 2 + i) as u64, LIST'))


def scoped_cleanup(root):
    root=Path(root)
    p=root/'novaOS/user/novaauth/service/src/bin/common/shell_verifier.rs'
    s=p.read_text();old='        drop[..8].copy_from_slice(b"NXPKDROP");'
    if s.count(old)!=1:raise ValueError('nettoyage session prive')
    p.write_text(s.replace(old,old+'\n        drop[8..40].copy_from_slice(&id);'))
    p=root/'novaOS/user/novaauth/service/exec_package_read.rs'
    s=p.read_text();old='b"NXPKDROP"=>{self.upload=None;self.executable_request=None;Ok(())},'
    if s.count(old)!=1:raise ValueError('nettoyage stockage prive')
    p.write_text(s.replace(old,'''b"NXPKDROP"=>{
                if q[8..40]==[0;32] || q[40..]!=[0;88] {return Err(rt::DENIED);}
                if self.executable_request.as_ref().is_some_and(|r|r[24..56]==q[8..40]) {
                    self.upload=None; self.executable_request=None;
                }
                Ok(())
            },'''))


def shell_probe(root):
    root=Path(root)
    p=root/'components/shell/main.rs';s=p.read_text()
    old='fn session() -> rt::Result<()> {'
    if s.count(old)!=1:raise ValueError('entree nsh privee R2')
    s=s.replace(old,'''#[path="../../novaOS/user/novaauth/service/administration_bob_shell.rs"]
mod administration_bob;
fn session() -> rt::Result<()> {
    if rt::config(2)==24 {return administration_bob::session();}''')
    p.write_text(s)
    p=root/'components/shell/session_auth_lab.rs';s=p.read_text()
    # Bob draine uniquement les canaux de SES enfants. Le banc observe la copie
    # IPC côté noyau ; aucun octet de Bob n'est rendu sur le terminal d'Alice.
    s=s.replace('let trace = option_env!("NOVA_DISPLAY_TRACE") == Some("1");',
                'let trace = option_env!("NOVA_DISPLAY_TRACE") == Some("1") && rt::config(2)!=24;')
    s=s.replace('if failure.is_none() && !text.is_empty() {',
                'if failure.is_none() && !text.is_empty() && rt::config(2)!=24 {')
    p.write_text(s)


def shared_sessions(root):
    """Une seule table de sessions dans le vérificateur, sans allocation heap."""
    root=Path(root);p=root/'novaOS/user/novaauth/service/src/bin/common/shell_verifier.rs';s=p.read_text()
    changes=[
        ('struct Login {', "struct Login<'a> {"),
        ('    sessions: Sessions,', "    sessions: &'a core::cell::RefCell<Sessions>,"),
        ('impl Login {', "impl<'a> Login<'a> {"),
        ('    fn from_launcher(slot: usize) -> Self {', "    fn from_launcher(slot: usize, sessions: &'a core::cell::RefCell<Sessions>) -> Self {"),
        ('            sessions: Sessions::new(),','            sessions,'),
        ('            .sessions\n            .open(', '            .sessions.borrow_mut()\n            .open('),
        ('self.sessions.inspect(', 'self.sessions.borrow_mut().inspect('),
        ('self.sessions.disconnect(', 'self.sessions.borrow_mut().disconnect('),
        ('    let mut logins = [Login::from_launcher(0), Login::from_launcher(1)];',
         '    let sessions=core::cell::RefCell::new(Sessions::new());\n    let mut logins = [Login::from_launcher(0,&sessions), Login::from_launcher(1,&sessions)];'),
    ]
    for old,new in changes:
        if s.count(old)!=1:raise ValueError('ancrage table commune R2 : '+old)
        s=s.replace(old,new)
    p.write_text(s)


def startup_words(root):
    """Les mots duplex appartiennent au bloc privé, pas à l'accesseur historique."""
    root=Path(root);p=root/'novaOS/user/novaauth/service/src/bin/common/shell_verifier.rs';s=p.read_text()
    s=s.replace('fn port(&self, n:usize)->u64 { rt::config(n + self.slot * 21) }',
        'fn port(&self, n:usize)->u64 { assert!((15..=20).contains(&n)); duplex_word(n + self.slot * 21) }')
    s=s.replace('client: identity(11 + slot * 21),','client: duplex_identity(11 + slot * 21),')
    s=s.replace('rt::config(36) == 0','duplex_word(36) == 0')
    s+='''
fn duplex_word(n:usize)->u64 {
    assert!((11..=20).contains(&n) || (32..=41).contains(&n));
    unsafe { (&*(0x401000 as *const core::sync::atomic::AtomicU64).add(n))
        .load(core::sync::atomic::Ordering::Acquire) }
}
fn duplex_identity(offset:usize)->[u8;32] {
    assert!(offset==11 || offset==32);
    let mut id=[0;32];
    for i in 0..4 {id[i*8..i*8+8].copy_from_slice(&duplex_word(offset+i).to_le_bytes());}
    id
}
'''
    p.write_text(s)


def isolate_closed_route(root):
    """Une panne de conversation n'arrête pas le vérificateur de l'autre session."""
    p=Path(root)/'novaOS/user/novaauth/service/src/bin/common/shell_verifier.rs';s=p.read_text()
    s=s.replace('    let mut last_tags = [0;2];','    let mut last_tags = [0;2];\n    let mut dead = [false;2];')
    s=s.replace('''        channels[slot].poll(login);
        let m = match rt::receive_for(login.port(15), 2) {''','''        if dead[slot] {
            // Aucun acquittement de retrait n'est inventé. Une fermeture dont
            // l'effecteur ne répond plus reste retiring, tandis qu'Alice continue.
            let _ = login.maintain();
            store_seq=login.store_seq; relay_seq=login.relay_seq;
            match rt::receive_for(rt::config(1),2) {
                Err(rt::TIMED_OUT|rt::WOULD_BLOCK)=>(),
                Ok(m) if m.kind==1 && m.len==1 && m.payload[0]==0=>return Ok(()),
                _=>return Err(rt::CANCELLED),
            }
            continue;
        }
        channels[slot].poll(login);
        let m = match rt::receive_for(login.port(15), 2) {
            Err(rt::DENIED|rt::CLOSED|rt::REVOKED) => {
                dead[slot]=true;
                login.verifier.disconnect(login.route);
                if let Some((id,_))=login.live {let _=login.close(id);}
                store_seq=login.store_seq; relay_seq=login.relay_seq;
                marker(b"NAR2 route auth fermee autres routes preservees");
                continue;
            }''')
    s=s.replace('''            outcome => outcome?,
        }
        store_seq=login.store_seq; relay_seq=login.relay_seq;''','''            Err(rt::DENIED|rt::CLOSED|rt::REVOKED) => {
                dead[slot]=true;
                login.verifier.disconnect(login.route);
                if let Some((id,_))=login.live {let _=login.close(id);}
                marker(b"NAR2 route reponse fermee autres routes preservees");
            },
            outcome => outcome?,
        }
        store_seq=login.store_seq; relay_seq=login.relay_seq;''')
    p.write_text(s)


def route_observer(root):
    """Publication de mesure uniquement, sans syscall ni droit invité nouveau."""
    p=Path(root)/'novaOS/kernel/src/sched/process/ipc/session_relay.rs';s=p.read_text()
    old='''                limit: deadline,
            });'''
    if s.count(old)!=1:raise ValueError('liaison de session R2 ambiguë')
    s=s.replace(old,old+'''
            let instance=s.processes.get(r.shell).instance.identity(crate::boot_id::current()).ok_or(REVOKED)?.encode();
            let mut record=[0u8;80];record[..8].copy_from_slice(b"NXR2SES1");
            record[8..16].copy_from_slice(&r.shell.number.to_le_bytes());
            record[16..48].copy_from_slice(&instance);record[48..].copy_from_slice(&id);
            nova_r2_session_observe(record.as_ptr(),record.len());''')
    s+='''
#[inline(never)]
#[unsafe(no_mangle)]
pub extern "C" fn nova_r2_session_observe(frame:*const u8,len:usize) {
    core::hint::black_box((frame,len));
}
''';p.write_text(s)


def output_endpoint_closed(root):
    """La fin de preuve suit la destruction réelle de l'endpoint, sans changer IPC."""
    root=Path(root);p=root/'novaOS/kernel/src/sched/process/ipc/mod.rs';s=p.read_text()
    old='        self.ends.remove(ep.slot());'
    if s.count(old)!=1:raise ValueError('destruction endpoint ambiguë')
    p.write_text(s.replace(old,old+'\n        self.output_probe.endpoint_closed(ep);'))
    p=root/'novaOS/kernel/src/sched/process/ipc/administration_output_probe.rs';s=p.read_text()
    old='    pub(super) fn delivered('
    if s.count(old)!=1:raise ValueError('sonde sortie ambiguë')
    s=s.replace(old,'''    pub(super) fn endpoint_closed(&mut self, endpoint:Reference) {
        for i in 0..SLOTS {
            if self.0[i].is_some_and(|v|v.endpoint==endpoint) {self.clear(i);}
        }
    }
'''+old);p.write_text(s)
