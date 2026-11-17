//! Client du temporaire pour une instance ayant délégué sa vue à un travailleur.
//! Pendant la publication, le demandeur réserve les deux slots PATH_FRAGMENT
//! de sa session à ce travailleur. TMP_WORKER limite les enfants en attente à un.
use nova_cli::{protocol as fs, runtime as rt};
#[allow(dead_code)]
#[path = "../../../novaOS/abi/tmp.rs"]
pub mod tmp;

pub struct Private(pub u64);
impl Private {
    pub fn send(&self, mut q: fs::Frame) -> Result<fs::Frame, u64> {
        rt::value(rt::call(
            tmp::CALL,
            self.0,
            q.as_mut_ptr() as u64,
            q.len() as u64,
        ))?;
        Ok(q)
    }
    pub fn named(&self, op: u8, name: &[u8], flags: u8) -> Result<fs::Frame, u64> {
        if name.is_empty() || name.len() > fs::PATH_MAX {
            return Err(rt::LIMIT);
        }
        if op == fs::PATH_FRAGMENT || name.len() > fs::FRAGMENT {
            let slot = if op == fs::PATH_FRAGMENT { flags } else { 0 };
            for (i, part) in name.chunks(fs::FRAGMENT).enumerate() {
                let mut q = fs::frame(fs::PATH_FRAGMENT, 1, 0);
                q[2] = slot;
                fs::size(&mut q, part.len());
                fs::set(&mut q, 24, (i * fs::FRAGMENT) as u64);
                fs::set(&mut q, 32, name.len() as u64);
                q[fs::PAYLOAD..fs::PAYLOAD + part.len()].copy_from_slice(part);
                let reply = self.send(q)?;
                if op == fs::PATH_FRAGMENT && (i + 1) * fs::FRAGMENT >= name.len() {
                    return Ok(reply);
                }
            }
        }
        let mut q = fs::frame(op, 1, 0);
        q[2] = flags;
        if name.len() <= fs::FRAGMENT {
            fs::size(&mut q, name.len());
            q[fs::PAYLOAD..fs::PAYLOAD + name.len()].copy_from_slice(name);
        }
        self.send(q)
    }

    fn publish_file(&self, name: &[u8]) -> Result<(), u64> {
        let pid = rt::value(rt::call(rt::PROCESS_ID, 0, 0, 0))?;
        // Choisir un composant racine distinct du premier composant de la destination.
        // Une collision avec une préparation ancienne conserve son contenu.
        let mut stage = None;
        for i in 0..128 {
            let candidate = alloc::format!(".mktemp-{pid:x}-{i:x}").into_bytes();
            if name.split(|&c| c == b'/').any(|part| part == candidate) {
                continue;
            }
            match self.named(fs::MKDIR_AT, &candidate, 0) {
                Ok(_) => {
                    stage = Some(candidate);
                    break;
                }
                Err(fs::EXISTS) => continue,
                Err(e) => return Err(e),
            }
        }
        let stage = stage.ok_or(fs::BUSY)?;
        let file = [stage.as_slice(), b"/data"].concat();
        let opened = match self.named(fs::OPEN_AT, &file, fs::CREATE | fs::WRITABLE) {
            Ok(f) => f,
            Err(fs::UNCERTAIN) => return Err(fs::UNCERTAIN),
            Err(e) => {
                self.named(fs::RMDIR, &stage, 0)?;
                return Err(e);
            }
        };
        self.send(fs::frame(fs::CLOSE, 1, fs::word(&opened, 16)))?;
        self.named(fs::PATH_FRAGMENT, &file, 0)?;
        self.named(fs::PATH_FRAGMENT, name, 1)?;
        match self.send(fs::frame(fs::RENAME_AT, 1, 0)) {
            Ok(_) => (),
            Err(fs::UNCERTAIN) => return Err(fs::UNCERTAIN),
            Err(e) => {
                self.named(fs::UNLINK_AT, &file, 0)?;
                self.named(fs::RMDIR, &stage, 0)?;
                return Err(e);
            }
        }
        // La publication a déjà eu lieu, un échec de nettoyage ne doit jamais
        // provoquer une nouvelle création sous un autre nom.
        self.named(fs::RMDIR, &stage, 0)
            .map_err(|_| fs::UNCERTAIN)?;
        Ok(())
    }
    pub fn claim(&self, path: &[u8], directory: bool, dry: bool) -> Result<bool, u64> {
        let name = path.strip_prefix(b"tmp/").unwrap_or(path);
        if name.is_empty()
            || name.len() > fs::PATH_MAX
            || name.starts_with(b"/")
            || name.split(|&b| b == b'/').any(|s| s == b"..")
        {
            return Err(rt::BAD_ARGUMENT);
        }
        if dry {
            return match self.named(fs::LOOKUP_AT, name, 0) {
                Ok(_) => Ok(false),
                Err(fs::NOT_FOUND) => Ok(true),
                Err(e) => Err(e),
            };
        }
        let r = if directory {
            self.named(fs::MKDIR_AT, name, 0).map(|_| ())
        } else {
            self.publish_file(name)
        };
        match r {
            Ok(()) => Ok(true),
            Err(fs::EXISTS) => Ok(false),
            Err(e) => Err(e),
        }
    }
}
