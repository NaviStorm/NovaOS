"""Rendez-vous borné : la confirmation résidente précède l'authentification Bob.
Aucune confirmation, capacité ou mutation n'est fabriquée par ce rendez-vous.
"""
from pathlib import Path

def install(root):
    root=Path(root)
    def edit(name, old, new):
        p=root/name;s=p.read_text()
        if s.count(old)!=1:raise ValueError('ancrage rendez-vous absent/ambigu : '+name+' : '+old[:60])
        p.write_text(s.replace(old,new))
    channel='novaOS/user/novaauth/service/src/bin/common/administration_channel.rs'
    edit(channel,'    inbox: Inbox,','    inbox: Inbox,\n    pending: Option<(u64, [u8;128], [u8;32], u64)>,')
    edit(channel,'    bob_triggered: bool,','    bob_triggered: bool,\n    prepared: Option<([u8;16],[u8;32],[u8;32])>,')
    edit(channel,'            bob_triggered: false,','            bob_triggered: false,\n            prepared: None,')
    # Deux initialiseurs de Lane, tous deux privés au profil.
    p=root/channel;s=p.read_text();assert s.count('inbox: Inbox::new(),')==2
    p.write_text(s.replace('inbox: Inbox::new(),','inbox: Inbox::new(), pending: None,'))
    edit(channel,'            let m = match rt::receive_for(key, 0) {',PENDING+'\n            let m = match rt::receive_for(key, 0) {')
    edit(channel,'            let mut answer = [0; 128];','            let mut answer = [0; 128];\n            let mut defer=None;')
    edit(channel,'                answer = exchange(login, request.try_into().unwrap())?;',
         '                answer = exchange(login, request.try_into().unwrap())?;'+TRIGGER)
    edit(channel,'            lane.inbox.discard();','            lane.inbox.discard();\n            if result.is_ok() {\n                if let Some((sid, until))=defer {\n                    lane.pending=Some((m.tag,answer,sid,until));\n                    continue;\n                }\n            }')
    p=root/channel;p.write_text(p.read_text()+SEND)
    verifier='novaOS/user/novaauth/service/src/bin/common/shell_verifier.rs'
    edit(verifier,"    slot: usize,","    slot: usize,\n    r2_offered: bool,\n    r2_ready: Option<[u8;32]>,")
    edit(verifier,'            slot,','            slot, r2_offered:false, r2_ready:None,')
    edit(verifier,'        self.relay_offer(&offer, reply)', '        self.relay_offer(&offer, reply)?;\n        self.r2_offered=true;\n        Ok(())')
    edit(verifier,'                    reply[8..24].copy_from_slice(&owner);','                    reply[8..24].copy_from_slice(&owner);\n                    // NSHLIVE après réception du ticket : même route, session\n                    // réellement inspectée ; aucune nouvelle authentification.\n                    if login.slot==1 && login.r2_offered {\n                        login.r2_ready=Some(m.payload[8..40].try_into().unwrap());\n                    }')
    bob='novaOS/user/novaauth/service/administration_bob_shell.rs'
    edit(bob,'    mark(b"NAR2 Bob ticket vivant non consomme")?;',
         '    mark(b"NAR2 Bob ticket vivant non consomme")?;\n    login.authenticate()?; // session existante : NSHLIVE seulement, aucun nouveau bail')
    import runpy
    runpy.run_path(str(root/'novaOS/user/novaauth/service/administration_tls_trace.py'))['install'](root)

PENDING = r'''            if let Some((tag, mut answer, sid, until))=lane.pending {
                // Une ancienne route remplacée a déjà détruit pending ci-dessus.
                let tick=now().unwrap_or(u64::MAX);
                let ready=peer.r2_ready.is_some_and(|id|peer.live.is_some_and(|(active,_)|active==id));
                if !ready && tick<until {continue;}
                lane.pending=None;
                let valid=if tick>=until {Err(rt::TIMED_OUT)}
                    else {login.inspected_binding(sid).map(|_|())};
                if let Err(code)=valid {answer.fill(0);answer[..8].copy_from_slice(&code.to_le_bytes());}
                send_answer(key,tag,&answer);
                continue;
            }'''
TRIGGER = r'''
                if !self.bob_triggered && login.slot==0 && peer.slot==1
                    && matches!(admin_command::decode(request),
                        Ok(admin_command::Command::Prepare(admin_command::Mutation::Disable(admin_command::Selector::Name("bob"))))) {
                    if answer[8..24]==[0;16] || answer[24..56]==[0;32] {return Err(rt::DENIED);}
                    let identity=(answer[8..24].try_into().unwrap(),answer[24..56].try_into().unwrap(),sid);
                    if let Some(first)=self.prepared {
                        if first!=identity {return Err(rt::DENIED);}
                        // La seconde préparation appartient au véritable ELF
                        // de confirmation déjà chargé. Elle reste sans mutation.
                        let mut event=[0;128];event[..8].copy_from_slice(b"NXR2GO01");
                        event[8..40].copy_from_slice(&peer.route.client);
                        event[40..72].copy_from_slice(&login.route.client);
                        event[72..88].copy_from_slice(&answer[8..24]);
                        event[88..120].copy_from_slice(&answer[24..56]);
                        self.bob_triggered=true;
                        rt::send_for(peer.port(16),0x4e58523253544152,&event,25)?;
                        // Même borne que l'attente client (5000 ticks), réduite
                        // à l'échéance de la capacité ; jamais de renouvellement.
                        defer=Some((sid,now()?.saturating_add(20_000).min(meta[29])));
                    } else {
                        if peer.live.is_some() {return Err(rt::DENIED);}
                        self.prepared=Some(identity);
                        let mut event=[0;128];event[..8].copy_from_slice(b"NXR2AU01");
                        event[8..40].copy_from_slice(&peer.route.client);
                        event[40..72].copy_from_slice(&login.route.client);
                        event[72..88].copy_from_slice(&answer[8..24]);
                        event[88..120].copy_from_slice(&answer[24..56]);
                        rt::send_for(peer.port(16),0x4e58523253544152,&event,25)?;
                    }
                }
'''
SEND = r'''
fn send_answer(key:u64,tag:u64,answer:&[u8;128]) {
    for offset in (0..answer.len()).step_by(admin_wire::PAYLOAD) {
        let Ok(packet)=admin_wire::frame(tag,Direction::Response,answer,offset) else {break;};
        if rt::send_for(key,tag,&packet,25).is_err() {break;}
    }
}
'''
