//! Le lecteur est attesté avant d'admettre le producteur. Une seule image en
//! attente par session. La première admission conserve un reçu et un droit WAIT.
use super::*;

pub(super) struct Reader {
    control: u64,
    root: Reference,
    head: [u8; 32],
    binding: exec::Binding,
    image: Option<(user::SealedImage, Vec<u8>)>,
    deadline: u64,
}

pub(super) fn prepare(
    s: &Core,
    r: &mut Route,
    control: u64,
    head: [u8; 32],
    binding: exec::Binding,
    image: user::SealedImage,
    config: Vec<u8>,
    lease_end: u64,
) -> Res<u64> {
    if r.reader.is_some() || r.pipe_root.is_some_and(|id| s.ipc.node(id).is_some()) {
        return Err(WOULD_BLOCK);
    }
    // Cette réservation non exécutable expire avec la session. Le ticket remis
    // après admission du producteur a sa propre échéance courte.
    let deadline = lease_end.saturating_mul(u64::from(HZ)) / 1000;
    let root = s.ipc.pipe_check(r.shell, control, None)?.id;
    if option_env!("NOVA_DISPLAY_TRACE") == Some("1") {
        hal::log(format_args!(
            "NXPIPE lecteur mesure tick={} limite={deadline}",
            ticks()
        ));
    }
    r.pipe_root = Some(root);
    r.reader = Some(Reader {
        control,
        root,
        head,
        binding,
        image: Some((image, config)),
        deadline,
    });
    Ok(0)
}

pub(super) fn writer(
    s: &Core,
    r: &Route,
    control: u64,
    head: [u8; 32],
    binding: exec::Binding,
) -> Res<()> {
    let v = r.reader.as_ref().ok_or(DENIED)?;
    if option_env!("NOVA_DISPLAY_TRACE") == Some("1") {
        hal::log(format_args!(
            "NXPIPE preparation producteur tick={} limite={} controle={} etat={} titulaire={} domaine={} session={}",
            ticks(),
            v.deadline,
            control == v.control,
            head == v.head,
            v.binding.holder == binding.holder,
            v.binding.domain == binding.domain,
            v.binding.session == binding.session
        ));
    }
    if control != v.control
        || head != v.head
        || v.binding.session != binding.session
        || v.binding.domain != binding.domain
        || v.binding.holder != binding.holder
        || v.image.is_none()
        || v.root != s.ipc.pipe_check(r.shell, control, None)?.id
    {
        return Err(DENIED);
    }
    if ticks() >= v.deadline {
        return Err(TIMED_OUT);
    }
    Ok(())
}

pub(super) fn take(s: &mut Core, r: &mut Route, q: &[u8]) -> Res<u64> {
    let v = r.reader.as_mut().ok_or(DENIED)?;
    if q[8..40] != v.binding.session
        || q[40..72] != v.head
        || word(q, 72) != v.control
        || q[80..].iter().any(|b| *b != 0)
        || !pipe_admission::matches_ticket(s, r.shell, v.root)
    {
        return Err(DENIED);
    }
    if ticks() >= v.deadline {
        return Err(TIMED_OUT);
    }
    let (image, config) = v.image.take().ok_or(REFUSED)?;
    exec::issue_authenticated(
        s,
        r.issuer,
        r.shell,
        v.binding,
        image,
        config,
        v.deadline.min(ticks().saturating_add(5 * u64::from(HZ))),
        2,
        v.control,
        [0; 3],
    )
}

pub(super) fn abandon(s: &mut Core, r: &mut Route) {
    if let Some(v) = r.reader.take() {
        pipe_admission::fail_ticket(s, r.shell, v.root);
    }
}

pub(super) fn tick(s: &mut Core, r: &mut Route) {
    if r.reader
        .as_ref()
        .is_some_and(|v| ticks() >= v.deadline || s.ipc.node(v.root).is_none())
    {
        if option_env!("NOVA_DISPLAY_TRACE") == Some("1") {
            hal::log(format_args!("NXPIPE abandon reservation tick={}", ticks()));
        }
        abandon(s, r);
    }
}

pub(in crate::sched::process::ipc) fn attach(
    s: &mut Core,
    owner: Pid,
    child: Pid,
    role: u8,
    control: u64,
) -> Res<()> {
    let v = s
        .processes
        .relay
        .as_ref()
        .and_then(|r| r.reader.as_ref())
        .ok_or(REVOKED)?;
    if v.control != control || ticks() >= v.deadline {
        return Err(REVOKED);
    }
    let root = v.root;
    let outcome = pipe_admission::attach_ticket(s, owner, child, control, root, role == 2);
    if option_env!("NOVA_DISPLAY_TRACE") == Some("1") {
        hal::log(format_args!(
            "NXPIPE raccord pid={} role={} issue={:?}",
            child.number, role, outcome
        ));
    }
    outcome?;
    if role == 2 {
        s.processes.relay.as_mut().unwrap().reader = None;
    }
    Ok(())
}
