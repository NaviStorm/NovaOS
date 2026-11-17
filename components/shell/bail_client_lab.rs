//! Témoins de durée réelle, exécutés seulement dans le profil QEMU --bail.
use super::{Login, rt};
use nova_cli::{fileapi::Client, protocol as w};

pub(super) struct Held {
    sid: [u8; 32],
    route: [u8; 32],
    read: u64,
    ticket: u64,
    writer: u64,
    wait: u64,
    child: u64,
}

impl Login {
    pub(super) fn await_expiry(&mut self, fs: &mut Client, path: &str) -> rt::Result<()> {
        let read = fs.open_at(0, path.as_bytes(), 0x80 | w::READABLE)?;
        let mut byte = [0];
        if fs.read(read, 0, &mut byte)? != 1 {
            return Err(rt::REFUSED);
        }
        // L'enfant lit son vrai stdin. Le parent conserve la feuille d'écriture.
        let key = self.ticket("cat", &["-"], 5000, 5, 0)?;
        rt::value(rt::call(super::super::tty::SPAWN, super::super::tty::EXEC_TICKET, key, 0))?;
        let (child, wait) = Self::receipt(key)?;
        rt::value(rt::call(rt::IPC_CLOSE, key, 0, 0))?;
        let writer = rt::value(rt::call(rt::IPC_NOTIFY, rt::config(16), child, 0x4e58_4f55_5402))?;
        let ticket = self.ticket("temoin", &[], 5000, 1, 0)?;
        self.held = Some(Held { sid: self.session.ok_or(rt::DENIED)?, route: self.identity,
                                read, ticket, writer, wait, child });
        let first = fs.open_at(0, path.as_bytes(), 0x80 | w::READABLE)?;
        if rt::receive_for(rt::config(17), 15000).err() != Some(rt::TIMED_OUT) {
            return Err(rt::REFUSED);
        }
        let access = fs.read(first, 0, &mut byte);
        let _ = fs.close(first);
        super::super::console::write(super::super::tty::OUTPUT,
            &alloc::format!("NXBAIL ancien handle lecture={}\n", access.err().unwrap_or(0)))?;
        if access != Err(rt::REVOKED) { return Err(rt::REFUSED); }
        Ok(())
    }

    pub(super) fn close_race(&mut self) -> rt::Result<()> {
        let sid = self.session.ok_or(rt::DENIED)?;
        let arm = self.request(b"NXBRACE1", &sid)?;
        let limit = u64::from_le_bytes(arm[8..16].try_into().unwrap());
        let current = u64::from_le_bytes(arm[16..24].try_into().unwrap());
        let ticks = limit.checked_sub(current + 5000).ok_or(rt::TIMED_OUT)? / 4;
        if rt::receive_for(rt::config(17), ticks).err() != Some(rt::TIMED_OUT) {
            return Err(rt::REFUSED);
        }
        self.close()?;
        self.request(b"NSHCLOS1", &sid)?;
        if self.request(b"NSHLIVE1", &sid).err() != Some(rt::DENIED) { return Err(rt::REFUSED); }
        // Une commande ordinaire ne doit pas ouvrir la session suivante.
        self.explicit_login_required = true;
        Ok(())
    }

    pub(super) fn old_grants(&mut self, fs: &mut Client) -> rt::Result<()> {
        let h = self.held.take().ok_or(rt::DENIED)?;
        if self.session == Some(h.sid) || self.identity != h.route {
            return Err(rt::REFUSED);
        }
        // Deux requêtes sont en vol avant la première lecture. Le dernier ID clos
        // ne peut retirer le nouveau bail, même si la seconde arrive tardivement.
        let mut close = [0; 128];
        close[..8].copy_from_slice(b"NSHCLOS1");
        close[8..40].copy_from_slice(&h.sid);
        let first = self.tag.checked_add(1).ok_or(rt::LIMIT)?;
        self.tag = first.checked_add(1).ok_or(rt::LIMIT)?;
        rt::send_for(rt::config(16), first, &close, 5000)?;
        // Le shell présente sa seconde demande avant de lire la première réponse.
        // Le rendez-vous de la première réponse expire au bout de 5000 ticks.
        rt::send_for(rt::config(16), self.tag, &close, 12000)?;
        let ack = rt::receive_for(rt::config(17), 5000)?;
        if ack.kind != 1 || ack.tag != self.tag || ack.len != 128 || ack.payload[..8] != [0;8] {
            return Err(rt::REFUSED);
        }
        if self.request(b"NSHLIVE1", &h.sid).err() != Some(rt::DENIED) { return Err(rt::REFUSED); }
        let read = fs.read(h.read, 0, &mut [0]).err().unwrap_or(0);
        let _ = fs.close(h.read);
        let spawn = rt::call(super::super::tty::SPAWN, super::super::tty::EXEC_TICKET, h.ticket, 0).status;
        let _ = rt::call(rt::IPC_CLOSE, h.ticket, 0, 0);
        let send = nova_cli::process::pipe_write(h.writer, b"x").err().map_or(0, |(_, e)| e);
        let _ = rt::call(rt::IPC_CLOSE, h.writer, 0, 0);
        let (fault, status) = nova_cli::process::wait(h.wait)?;
        self.authenticate()?;
        super::super::console::write(super::super::tty::OUTPUT, &alloc::format!(
            "NXBAIL anciens lecture={read} ticket={spawn} flux={send} enfant={} statut={status} faute={} doubles=2 route=identique session=distincte\n",
            h.child, fault as u8))?;
        if !matches!(read, rt::REVOKED | rt::DENIED) ||
           !matches!(spawn, rt::REVOKED | rt::DENIED | rt::TIMED_OUT) ||
           !matches!(send, rt::REVOKED | rt::DENIED | rt::CLOSED) || fault {
            return Err(rt::REFUSED);
        }
        Ok(())
    }
}
