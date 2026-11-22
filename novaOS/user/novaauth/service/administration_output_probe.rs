//! Observation privée R2 par le débogueur QEMU, jamais un canal utilisateur.
//! Les octets ne sont observés qu'après copie et consommation IPC effectives.
use super::*;

const SLOTS: usize = 8; // Deux routes, deux enfants, stdout et stderr.
const WIRE: usize = 272;
#[derive(Clone, Copy)]
struct Stream {
    root: Reference,
    endpoint: Reference,
    receiver: Pid,
    child: Pid,
    receiver_instance: [u8; 32],
    child_instance: [u8; 32],
    session: [u8; 32],
    fd: u8,
    sequence: u64,
}

pub(super) struct Probe([Option<Stream>; SLOTS]);
const _: () = assert!(core::mem::size_of::<Probe>() <= 1536);
impl Probe {
    pub(super) const fn new() -> Self {
        Self([None; SLOTS])
    }
    fn emit(stream: &Stream, event: u8, bytes: &[u8]) {
        // Toute longueur a déjà été validée par IPC. Aucune troncature.
        assert!(bytes.len() <= 128);
        let mut frame = [0u8; WIRE];
        frame[..8].copy_from_slice(b"NXR2OUT1");
        frame[8..10].copy_from_slice(&1u16.to_le_bytes());
        frame[10..12].copy_from_slice(&(WIRE as u16).to_le_bytes());
        frame[12] = event;
        frame[13] = stream.fd;
        frame[14..16].copy_from_slice(&(bytes.len() as u16).to_le_bytes());
        frame[16..24].copy_from_slice(&stream.receiver.number.to_le_bytes());
        frame[24..32].copy_from_slice(&stream.child.number.to_le_bytes());
        frame[32..64].copy_from_slice(&stream.receiver_instance);
        frame[64..96].copy_from_slice(&stream.child_instance);
        frame[96..128].copy_from_slice(&stream.session);
        frame[128..136].copy_from_slice(&stream.endpoint.0.to_le_bytes());
        frame[136..144].copy_from_slice(&stream.sequence.to_le_bytes());
        frame[144..144 + bytes.len()].copy_from_slice(bytes);
        nova_r2_output_observe(frame.as_ptr(), frame.len());
    }
    fn clear(&mut self, at: usize) {
        if let Some(mut stream) = self.0[at].take() {
            stream.sequence = stream.sequence.checked_add(1).expect("séquence sonde R2");
            Self::emit(&stream, 2, &[]);
        }
    }
    pub(super) fn delivered(
        &mut self,
        receiver: Pid,
        endpoint: Reference,
        kind: u32,
        msg: Message,
    ) {
        if kind != 1 {
            return;
        }
        let Some(stream) = self
            .0
            .iter_mut()
            .flatten()
            .find(|v| v.receiver == receiver && v.endpoint == endpoint)
        else {
            return;
        };
        stream.sequence = stream.sequence.checked_add(1).expect("séquence sonde R2");
        Self::emit(stream, 1, &msg.bytes[..msg.len as usize]);
    }
}

// Le point d'observation n'est pas un appel ABI : aucune application ne peut
// l'invoquer. Le débogueur lit les registres d'arguments et 272 octets de pile.
// Il ne modifie aucune mémoire, réponse, ordonnance ou décision de sécurité.
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn nova_r2_output_observe(frame: *const u8, len: usize) {
    core::hint::black_box((frame, len));
}

pub(super) fn attach(
    s: &mut Core,
    receiver: Pid,
    child: Pid,
    session: [u8; 32],
    roots: [Option<Reference>; 2],
    reads: [u64; 2],
) -> Res<()> {
    if !session_relay::owns_session(s, receiver, session)
        || !exec::session_child(s, receiver, child, session)
    {
        return Err(DENIED);
    }
    let identity = |p: Pid| -> Res<[u8; 32]> {
        let p = s.processes.get(p);
        p.instance
            .identity(crate::boot_id::current())
            .map(|v| v.encode())
            .ok_or(REVOKED)
    };
    let receiver_instance = identity(receiver)?;
    let child_instance = identity(child)?;
    let mut pending = [None; 2];
    for fd in 0..2 {
        let Some(root) = roots[fd] else {
            continue;
        };
        let writer = s.ipc.node(root).ok_or(REVOKED)?;
        let reader = s.ipc.lookup(receiver, reads[fd], RECEIVE)?;
        let endpoint = s.ipc.ends.get(writer.endpoint.slot());
        if writer.rights != SEND
            || writer.holder as usize != child.slot
            || endpoint.root != root
            || endpoint.owner != child.number
            || reader.parent != root
            || reader.rights != RECEIVE
            || reader.endpoint != writer.endpoint
        {
            return Err(DENIED);
        }
        pending[fd] = Some(Stream {
            root,
            endpoint: writer.endpoint,
            receiver,
            child,
            receiver_instance,
            child_instance,
            session,
            fd: fd as u8 + 1,
            sequence: 0,
        });
    }
    // Les références générationnelles interdisent de confondre un slot réutilisé.
    for i in 0..SLOTS {
        if s.ipc.output_probe.0[i].is_some_and(|v| s.ipc.node(v.root).is_none()) {
            s.ipc.output_probe.clear(i);
        }
    }
    if pending.iter().flatten().any(|v| {
        s.ipc
            .output_probe
            .0
            .iter()
            .flatten()
            .any(|old| old.endpoint == v.endpoint)
    }) {
        return Err(REFUSED);
    }
    if s.ipc.output_probe.0.iter().filter(|v| v.is_none()).count()
        < pending.iter().flatten().count()
    {
        return Err(LIMIT);
    }
    for stream in pending.into_iter().flatten() {
        let at = s
            .ipc
            .output_probe
            .0
            .iter()
            .position(Option::is_none)
            .unwrap();
        s.ipc.output_probe.0[at] = Some(stream);
        Probe::emit(&stream, 0, &[]);
    }
    Ok(())
}

pub(super) fn forget(s: &mut Core, child: Pid) {
    for i in 0..SLOTS {
        if s.ipc.output_probe.0[i].is_some_and(|v| v.child == child) {
            s.ipc.output_probe.clear(i);
        }
    }
}

// Une erreur de sonde invalide la preuve, jamais la décision du lanceur.
pub(super) fn invalid() {
    let mut frame = [0u8; WIRE];
    frame[..8].copy_from_slice(b"NXR2OUT1");
    frame[12] = 255;
    nova_r2_output_observe(frame.as_ptr(), frame.len());
}
