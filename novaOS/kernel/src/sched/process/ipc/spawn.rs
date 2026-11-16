//! Le catalogue associe un numéro à un ELF immuable. Le service des fichiers
//! interprète seul la sélection d'objet. Le second droit WAIT est réservé au lecteur du pipe.
use super::*;
use crate::terminal as tty;
#[path = "document_launch.rs"]
mod document_launch;
const IMAGES: &[(&[u8], u64, bool, bool)] = include!(concat!(env!("OUT_DIR"), "/cli_catalog.rs"));

// Seule une entrée de ce catalogue immutable permet d’obtenir ce type.
#[derive(Clone, Copy)]
pub(super) struct Application(&'static [u8], u64);
impl Application {
    fn entry(index: usize) -> Res<(Self, bool, bool)> {
        IMAGES
            .get(index)
            .filter(|entry| entry.1 != 0)
            .map(|&(image, app, restricted, docs)| (Self(image, app), restricted, docs))
            .ok_or(BAD_ARGUMENT)
    }

    pub(super) fn admission(self, ports: u64, files: bool) -> Res<supervision::Admission> {
        let mut plan = [0; 24];
        for (chunk, value) in plan
            .chunks_exact_mut(8)
            .zip([self.1, ports, u64::from(files)])
        {
            chunk.copy_from_slice(&value.to_le_bytes());
        }
        supervision::decide(&plan)
    }

    pub(super) fn principal(self, data: &[u8], admission: supervision::Admission) -> Res<Child> {
        self.principal_in(data, admission, false)
    }

    fn principal_in(self, data: &[u8], admission: supervision::Admission, legacy_p8: bool) -> Res<Child> {
        let _guard = Guard::new();
        let child = self.launch(data, admission.workers(), None)?;
        // Les deux ELF de qualification historiques peuvent exécuter leur sonde
        // sans disque. Un lancement applicatif ordinaire requiert le service.
        let no_disk_probe = self.diskless_fixture()
            && with(|s| s.processes.temporary.diskless_profile());
        if !legacy_p8 && !no_disk_probe && let Err(status) = tmp::prepare(child.0) {
            retire_unpublished(child);
            return Err(status);
        }
        admission.bind(with(|s| s.processes.get(child.0).instance));
        Ok(child)
    }

    fn diskless_fixture(self) -> bool {
        #[cfg(feature = "thread-probe")]
        if self.1 == iw::WITNESS && core::ptr::eq(self.0, witness().0) {
            return true;
        }
        self.1 == iw::SAMPLE && core::ptr::eq(self.0, sample().0)
    }

    // Appelé sous la garde du lanceur ; les droits sont installés avant le premier quantum.
    pub(super) fn launch(self, data: &[u8], workers: bool, member: Option<Instance>) -> Res<Child> {
        let space = heap::with_pages(|m| UserSpace::new(m, self.0, data, None))
            .map_err(|e| launch_error(Error::Memory(e)))?;
        let child = publish_image(space, None, self.1, member).map_err(launch_error)?;
        with(|s| {
            let p = s.processes.get_mut(child.0);
            p.application_threads = true;
            p.worker_image = workers.then_some(self.0);
        });
        Ok(child)
    }
}

#[cfg(feature = "thread-probe")]
fn witness() -> Application {
    let (image, restricted, _) = Application::entry(15).expect("témoin admis");
    assert!(restricted && image.0.starts_with(b"\x7fELF"));
    image
}

#[cfg(feature = "thread-probe")]
pub(super) fn thread_witness() -> &'static [u8] {
    witness().0
}

#[cfg(feature = "thread-probe")]
pub(super) fn launch_thread_witness(data: &[u8]) -> Res<Child> {
    let app = witness();
    let workers = app.admission(0, false)?;
    let _gate = Guard::new();
    app.principal(data, workers)
}

pub(in crate::sched::process) fn spawn_program(number: u64, addr: u64, len: u64) -> Res<u64> {
    let _guard = Guard::new();
    with(|s| threads::status(s))?;
    let parent = with(|s| s.thread(s.current()).owner.unwrap());
    let authority = with(|s| s.processes.get(parent).terminal);
    if authority & tty::LAUNCH == 0 {
        return Err(DENIED);
    }
    if number == tty::PIPE_OPEN {
        if addr != 0 || len != 0 { return Err(BAD_ARGUMENT); }
        return with(|s| s.ipc.pipe_open(parent, authority & tty::QUALIFY != 0));
    }
    use pipe::Role;
    let role = match number {
        tty::PIPE_CAT => Some(Role::Cat),
        tty::PIPE_WC => Some(Role::Wc),
        tty::PIPE_HEAD => Some(Role::Head),
        tty::PIPE_SORT => Some(Role::Sort),
        tty::PIPE_UNIQ => Some(Role::Uniq),
        n if n == Role::Sort as u64 => return Err(DENIED),
        _ => None,
    };
    let number = role.map_or(number, |r| r as u64);
    let stream = role.map(Role::reader);
    let check = |s: &Core, key: u64| -> Res<Option<Node>> {
        let p = s.processes.get(parent);
        if p.pipe_reader_wait.is_some() { return Err(LIMIT); }
        if stream == Some(true) {
            let writer = p.wait_right.ok_or(DENIED)?.target;
            return s.ipc.pipe_check(parent, key, Some(writer)).map(Some);
        }
        if p.wait_right.is_some() { return Err(LIMIT); }
        stream.map(|_| s.ipc.pipe_check(parent, key, None)).transpose()
    };
    let (image, fixture, has_documentation) = Application::entry(usize::try_from(number).map_err(|_| BAD_ARGUMENT)?)?;
    let mut bytes = [0; tty::HEADER + tty::ARG_MAX + 8];
    let n = usize::try_from(len).map_err(|_| BAD_ARGUMENT)?;
    let extension = usize::from(stream.is_some()) * 8;
    if n < tty::HEADER + extension || n > tty::HEADER + tty::ARG_MAX + extension { return Err(BAD_ARGUMENT); }
    let data = bytes
        .get_mut(..n)
        .filter(|b| b.len() >= tty::HEADER)
        .ok_or(BAD_ARGUMENT)?;
    let valid = with(|s| {
        heap::with_pages(|m| {
            let space = s.processes.get(parent).space.as_ref().unwrap();
            Ok(data.chunks_mut(DEBUG_MAX).enumerate().all(|(i, b)| {
                addr.checked_add((i * DEBUG_MAX) as u64)
                    .is_some_and(|p| space.copy_from_user(m, p, b))
            }))
        })
        .unwrap()
    });
    if !valid {
        return Err(BAD_ADDRESS);
    }
    let (data, trailer) = data.split_at_mut(n - extension);
    let pipe = if stream.is_some() { u64::from_le_bytes(trailer.try_into().unwrap()) } else { 0 };
    with(|s| check(s, pipe))?;
    let word = |i: usize| u64::from_le_bytes(data[i * 8..i * 8 + 8].try_into().unwrap());
    let (input, output, ports) = (word(0), word(1), word(2));
    let probe = matches!((role, word(3)),
        (Some(Role::Cat), tty::PIPE_PROBE_WAIT) | (Some(Role::Head), tty::PIPE_PROBE_REVOKE));
    if probe && authority & tty::QUALIFY == 0 {
        return Err(DENIED);
    }
    let revoke_before_read = role == Some(Role::Head) && word(3) == tty::PIPE_PROBE_REVOKE;
    // Le banc peut retirer OUTPUT à head pour exercer une vraie erreur console.
    let denied_output_probe = authority & tty::QUALIFY != 0 && stream == Some(true)
        && role == Some(Role::Head) && word(3) == DENIED && ports == u64::from(tty::ERROR);
    if word(4) != (data.len() - tty::HEADER) as u64
        || stream == Some(true) && (input != 0 || ports != u64::from(tty::ERROR | tty::OUTPUT) && !denied_output_probe)
        || stream == Some(false) && ports != u64::from(tty::ERROR)
        || ports & !u64::from(authority & (tty::INPUT | tty::OUTPUT | tty::ERROR)) != 0
        || (input == 0) != (output == 0)
        || (input != 0 && input == output)
    {
        return Err(BAD_ARGUMENT);
    }
    if fixture && authority & tty::QUALIFY == 0 {
        return Err(DENIED);
    }
    let documentation = stream.is_none() && document_launch::eligible(has_documentation, (input, output), ports, word(3), &data[tty::HEADER..]);
    data[..24].fill(0);
    let start = hal::timer::counter();
    drop(_guard);
    let workers = image.admission(ports, input != 0)?;
    let _guard = Guard::new();
    with(|s| threads::status(s))?;
    // L’appelant ne fournit aucun sélecteur de profil. Le lanceur de boot
    // attribue ce marqueur uniquement au shell du profil P8 explicite.
    let legacy_p8 = with(|s| s.processes.get(parent).legacy_p8_launcher);
    // L'admission et l'identité sont celles d'un ELF isolé. Aucun chemin de fichier
    // ni handle métier n’entre dans un lancement documentaire.
    let child = if documentation && !workers.workers() {
        let child = image.launch(data, false, None)?;
        workers.bind(with(|s| s.processes.get(child.0).instance));
        child
    } else {
        image.principal_in(data, workers, legacy_p8)?
    };
    let setup = with(|s| {
        let pid = child.0;
        let root = check(s, pipe)?;
        if input != 0 {
            let tx = s.ipc.derive(parent, input, pid, SEND.into())?;
            let rx = s.ipc.derive(parent, output, pid, RECEIVE.into())?;
            bind_grants(s, pid, [tx, rx])?;
            s.ipc.locals.get_mut(pid.slot).bootstrap = tx;
            heap::with_pages(|m| {
                s.processes
                    .get(pid)
                    .space
                    .as_ref()
                    .unwrap()
                    .write_data(m, 0, rx);
                Ok(())
            })
            .unwrap();
        }
        if let (Some(root), Some(role)) = (root, role) {
            let key = s.ipc.pipe_attach(root, pid, role)?;
            if authority & tty::QUALIFY != 0 {
                hal::log(format_args!("PIPE admission image={number} fichiers={} terminal={ports}", u8::from(input != 0)));
            }
            if revoke_before_read {
                // Le contrôleur du parent vient d'être revérifié sous la même garde.
                // Le premier RECEIVE utilisateur observera nécessairement le retrait.
                s.ipc.revoke(root);
                s.flush_ipc();
            }
            heap::with_pages(|m| {
                s.processes.get(pid).space.as_ref().unwrap().write_data(m, if role.reader() { 2 } else { 1 }, key);
                Ok(())
            }).unwrap();
        }
        s.processes.get_mut(pid).terminal = ports as u8 | (authority & tty::QUALIFY);
        let right = WaitRight { target: pid };
        if stream == Some(true) {
            s.processes.get_mut(parent).pipe_reader_wait = Some(right);
        } else {
            s.processes.get_mut(parent).wait_right = Some(right);
        }
        Ok(right.key())
    });
    match setup {
        Ok(key) => {
            let sample = authority & tty::QUALIFY != 0;
            if sample {
                let pages = with(|s| {
                    s.processes
                        .get(child.0)
                        .space
                        .as_ref()
                        .unwrap()
                        .page_counts()
                });
                hal::log(format_args!(
                    "CLI spawn image={number} pid={} cycles={} pages={}",
                    child.0.number,
                    hal::timer::counter() - start,
                    pages.0 + pages.1
                ));
                measure("cli_enfant");
            }
            core::mem::forget(child);
            Ok(key)
        }
        Err(e) => {
            retire_unpublished(child);
            Err(e)
        }
    }
}

fn export_identity(s: &Core, pid: Pid, key: Instance, address: u64) -> Res<u64> {
    let bytes = key
        .identity(crate::boot_id::current())
        .ok_or(REFUSED)?
        .encode();
    let space = s.processes.get(pid).space.as_ref().unwrap();
    let written = heap::with_pages(|m| Ok(space.copy_to_user(m, address, &bytes))).unwrap();
    written.then_some(iw::Identity::SIZE).ok_or(BAD_ADDRESS)
}

fn verify_identity(s: &Core, pid: Pid, key: Instance, address: u64) -> Res<()> {
    let expected = key.identity(crate::boot_id::current()).ok_or(REFUSED)?;
    let mut bytes = [0; iw::Identity::SIZE as usize];
    let space = s.processes.get(pid).space.as_ref().unwrap();
    let read = heap::with_pages(|m| Ok(space.copy_from_user(m, address, &mut bytes))).unwrap();
    if !read {
        return Err(BAD_ADDRESS);
    }
    (iw::Identity::decode(bytes) == expected)
        .then_some(())
        .ok_or(DENIED)
}

pub(in crate::sched::process) fn instance_call(op: u64, a: u64, b: u64, c: u64) -> Res<u64> {
    let _guard = Guard::new();
    let (pid, key, app, principal, worker_image) = with(|s| -> Res<_> {
        threads::status(s)?;
        let p = s.processes.get(s.thread(s.current()).owner.unwrap());
        Ok((
            p.pid,
            p.instance,
            s.processes.instances.app(p.instance).unwrap(),
            p.principal,
            p.worker_image,
        ))
    })?;
    match op {
        iw::INFO if b == 0 && c == 0 => match a {
            0 => Ok(key.serial),
            1 => Ok(app),
            _ => Err(BAD_ARGUMENT),
        },
        iw::INFO if a == 2 && c == iw::Identity::SIZE => with(|s| export_identity(s, pid, key, b)),
        iw::WORKER | tmp::wire::WORKER if c == iw::Identity::SIZE => {
            let image = worker_image.ok_or(DENIED)?;
            if !principal {
                return Err(DENIED);
            }
            with(|s| verify_identity(s, pid, key, a))?;
            if with(|s| { let p = s.processes.get(pid); p.wait_right.is_some() || p.pipe_reader_wait.is_some() }) {
                return Err(LIMIT);
            }
            let child = Application(image, app).launch(&b.to_le_bytes(), false, Some(key))?;
            if op == tmp::wire::WORKER &&
                let Err(e) = with(|s| tmp::delegate(s, pid, child.0)) {
                retire_unpublished(child);
                return Err(e);
            }
            let right = WaitRight { target: child.0 };
            with(|s| s.processes.get_mut(pid).wait_right = Some(right));
            core::mem::forget(child);
            Ok(right.key())
        }
        iw::OFFER_STOP if b == 0 && c == iw::Identity::SIZE => {
            if !principal || app == iw::SYSTEM || app == iw::SHELL {
                return Err(DENIED);
            }
            with(|s| verify_identity(s, pid, key, a))?;
            with(|s| s.processes.get_mut(pid).stop_offered = true);
            Ok(0)
        }
        iw::STOP | iw::OFFERED if c == iw::Identity::SIZE => with(|s| {
            let right = s
                .processes
                .get(pid)
                .wait_right
                .filter(|r| r.key() == a)
                .ok_or(DENIED)?;
            let p = s.processes.get(right.target);
            if !p.principal || !p.stop_offered || p.outcome.is_some() {
                return Err(DENIED);
            }
            if op == iw::OFFERED {
                return export_identity(s, pid, p.instance, b);
            }
            verify_identity(s, pid, p.instance, b)?;
            stop_process(s, right.target, Outcome::Exit(125), hal::timer::counter());
            Ok(0)
        }),
        _ => Err(BAD_ARGUMENT),
    }
}

pub(super) fn tmp_sample() -> Application {
    Application(include_bytes!(concat!(env!("OUT_DIR"), "/tmp_sample.bin")), iw::SAMPLE)
}

pub(super) fn sample() -> Application {
    Application::entry(iw::SAMPLE_INDEX)
        .expect("application de référence")
        .0
}
