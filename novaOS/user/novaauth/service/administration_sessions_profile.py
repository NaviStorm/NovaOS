"""Câblage R2 QEMU uniquement : conversations privées des sessions.

Les racines restent imputées au bénéficiaire. Ni configuration utilisateur ni
nom d'exécutable ne sélectionnent un pair. Les préimages R1 sont inchangées.
"""
from pathlib import Path


def duplex(root):
    root = Path(root)

    def edit(name, before, after):
        p = root / name
        s = p.read_text()
        if s.count(before) != 1:
            raise ValueError(f"ancrage duplex R2 absent ou ambigu : {name}: {before[:80]}")
        p.write_text(s.replace(before, after))

    base = 'novaOS/kernel/src/sched/process/ipc/'
    edit(base + 'service/shell_auth_lab.rs',
         '    let (ask, requests) = channel(s, m.client, Some(verifier), SEND)?;\n    let (reply, replies) = match channel(s, m.client, Some(verifier), RECEIVE) {\n        Ok(v) => v,\n        Err(e) => {\n            let root = s.ipc.lookup(m.client, ask, 0)?;\n            s.ipc.close(root);\n            return Err(e);\n        }\n    };',
         """    // Une conversation privée, sans GRANT/MANAGE : deux feuilles et
    // une seule racine au shell. Réserve sa quatrième racine à l'admission.
    let (ask, requests) = auth_lab::conversation(s, m.client, m.client, verifier)?;
    let (reply, replies) = (ask, requests);""")
    edit(base + 'service/auth_lab.rs',
         'fn conversation(s: &mut Core, owner: Pid, peer: Pid, fs: Pid)',
         'pub(super) fn conversation(s: &mut Core, owner: Pid, peer: Pid, fs: Pid)')
    edit(base + 'session_relay.rs', '    auth_port: u64,\n    relay_port: u64,\n    live:',
         '    auth_port: u64,\n    relay_port: u64,\n    authentication: Reference,\n    live:')
    edit(base + 'session_relay.rs', '    let shell_instance = live(s, shell)?.instance;',
         """    // Trouver la seule conversation de connexion créée par le noyau,
    // pas les mots de DATA que le shell peut modifier. Une ambiguïté refuse.
    let mut roots = s.ipc.ends.ids().filter_map(|i| {
        let ep = s.ipc.ends.get(i);
        let root = s.ipc.node(ep.root)?;
        if ep.owner != shell.number || root.rights != 0 { return None; }
        let has = |peer:Pid| s.ipc.nodes.ids().any(|j| {
            let n=s.ipc.nodes.get(j);
            n.parent==root.id && n.holder as usize==peer.slot && n.rights==SEND|RECEIVE
        });
        (has(shell) && has(issuer)).then_some(root.id)
    });
    let authentication=roots.next().ok_or(DENIED)?;
    if roots.next().is_some() { return Err(DENIED); }
    let shell_instance = live(s, shell)?.instance;""")
    edit(base + 'session_relay.rs', '        auth_port,\n        relay_port,\n        live:',
         '        auth_port,\n        relay_port,\n        authentication,\n        live:')
    edit(base + 'session_display_lab.rs',
         """            if end.owner != p.number
                || end.root != n.id
                || n.rights != SEND
                || !s.ipc.nodes.ids().any(|i| {
                    let v = s.ipc.nodes.get(i);
                    v.parent == n.id && v.holder as usize == r.issuer.slot && v.rights == RECEIVE
                })""",
         """            if end.owner != p.number
                || end.root != r.authentication
                || n.parent != r.authentication
                || n.rights != SEND|RECEIVE
                || s.ipc.node(r.authentication).is_none_or(|root| root.rights!=0)
                || !s.ipc.nodes.ids().any(|i| {
                    let v = s.ipc.nodes.get(i);
                    v.parent == r.authentication && v.holder as usize == r.issuer.slot && v.rights == SEND|RECEIVE
                })""")


def output_observer(root):
    """Sonde sans droit supplémentaire ; le collecteur est exclusivement hôte."""
    root = Path(root)
    base = 'novaOS/kernel/src/sched/process/ipc/'

    def edit(name, before, after):
        p = root / name
        source = p.read_text()
        if source.count(before) != 1:
            raise ValueError(f'ancrage observation R2 absent ou ambigu : {name}: {before[:80]}')
        p.write_text(source.replace(before, after))

    probe = root / (base + 'administration_output_probe.rs')
    if probe.exists():
        raise ValueError('sonde R2 déjà présente')
    probe.write_bytes((root / 'novaOS/user/novaauth/service/administration_output_probe.rs').read_bytes())
    edit(base+'mod.rs', 'mod traffic;', 'mod traffic;\nmod administration_output_probe;')
    edit(base+'mod.rs', '    metrics: metrics::Metrics,',
         '    metrics: metrics::Metrics,\n    output_probe: administration_output_probe::Probe,')
    edit(base+'mod.rs', '            metrics: metrics::Metrics::new(),',
         '            metrics: metrics::Metrics::new(),\n            output_probe: administration_output_probe::Probe::new(),')
    edit(base+'traffic.rs', '                Ok(kind as u64)',
         '                self.output_probe.delivered(pid, ep, kind, msg);\n                Ok(kind as u64)')
    # Observer après publication de session_origin ; conserver session_child.
    # La sonde ne décide jamais de l'issue du lancement.
    edit(base+'exec.rs',
         '                        s.processes.get_mut(child.0).session_origin=Some((owner,id));',
         '                        s.processes.get_mut(child.0).session_origin=Some((owner,id));\n                        super::session_relay::observe_display(s,owner,child.0);')
    edit(base+'session_relay.rs', 'pub(super) use display::attach as attach_display;',
         'pub(super) use display::attach as attach_display;\npub(super) use display::observe_child as observe_display;')
    p=root/(base+'session_display_lab.rs')
    p.write_text(p.read_text()+"""

pub(in crate::sched::process::ipc) fn observe_child(s: &mut Core, owner: Pid, child: Pid) {
    let capture = s.processes.relay.as_ref().and_then(|r| {
        let lease=r.live?;
        if r.shell!=owner {return None;}
        r.captures.iter().flatten().find(|c|c.child==child).map(|c| (lease.id,c.roots,c.reads))
    });
    let result = capture.ok_or(DENIED).and_then(|(sid,roots,reads)| {
        super::super::administration_output_probe::attach(s,owner,child,sid,roots,reads)
    });
    if result.is_err() {super::super::administration_output_probe::invalid();}
}
""")
    edit(base+'session_display_lab.rs', '    for c in r.captures.iter_mut().filter_map(Option::take) {',
         '''    for c in r.captures.iter_mut().filter_map(Option::take) {
        super::super::administration_output_probe::forget(s, c.child);''')
    # La sortie ne suffit pas à déclarer la collecte complète. Le banc exige
    # séparément le WAIT réel et les compteurs de restitution du scénario.
