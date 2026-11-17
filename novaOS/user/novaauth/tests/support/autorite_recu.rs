//! Banc de composant Unix. Une paire de sockets anonyme désigne le processus d'autorité.
//! Le protocole de cette fixture n'est jamais proposé aux processus de l'invité.
use super::*;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    cell::RefCell,
    io::{Read, Write},
    os::{fd::AsFd, unix::net::UnixStream},
    process::{Child, Command, Stdio},
    time::Duration,
};

#[derive(Serialize, Deserialize)]
enum Query {
    Head,
    Outcome(Id),
    Decide(Head, Head),
    New,
    Bind(Head),
    Receipt(Id),
}
#[derive(Serialize, Deserialize)]
pub struct SignedReceipt {
    pub head: Head,
    signature: Vec<u8>,
}
impl SignedReceipt {
    pub fn verify(&self, key: &VerifyingKey) -> bool {
        Signature::from_slice(&self.signature)
            .is_ok_and(|sig| key.verify_strict(&bytes(self.head), &sig).is_ok())
    }
}
#[derive(Serialize, Deserialize)]
enum Answer {
    Ok,
    Head(Option<Head>),
    Receipt(SignedReceipt),
    Conflict,
    Refused,
}

fn frame<T: Serialize>(sock: &mut UnixStream, value: &T) -> std::io::Result<()> {
    let data = serde_json::to_vec(value)?;
    sock.write_all(&(data.len() as u32).to_be_bytes())?;
    sock.write_all(&data)
}
fn read<T: DeserializeOwned>(sock: &mut UnixStream) -> std::io::Result<T> {
    let mut size = [0u8; 4];
    sock.read_exact(&mut size)?;
    let n = u32::from_be_bytes(size) as usize;
    if n > 16384 {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    let mut data = vec![0; n];
    sock.read_exact(&mut data)?;
    Ok(serde_json::from_slice(&data)?)
}
fn response_bytes(nonce: &[u8; 32], query: &Query, answer: &Answer) -> Vec<u8> {
    serde_json::to_vec(&("NovaAuth/fixture/answer/v1", nonce, query, answer)).unwrap()
}
pub fn serve() {
    let root = PathBuf::from(std::env::var_os("NOVA_TEST_LEDGER").unwrap());
    let sector = std::env::var("NOVA_TEST_SECTOR").unwrap().parse().unwrap();
    if std::env::var_os("NOVA_TEST_CREER").as_deref() == Some(std::ffi::OsStr::new("1")) {
        drop(device(&root, 16, sector, true));
        let mut key = [0; 32];
        File::open("/dev/urandom")
            .unwrap()
            .read_exact(&mut key)
            .unwrap();
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(root.with_extension("cle-test"))
            .unwrap();
        f.write_all(&key).unwrap();
        f.sync_all().unwrap();
    }
    let mut ledger = Ledger::new(device(&root, 16, sector, false));
    let secret: [u8; 32] = std::fs::read(root.with_extension("cle-test"))
        .unwrap()
        .try_into()
        .unwrap();
    let key = SigningKey::from_bytes(&secret);
    let mut socket = UnixStream::from(std::io::stdin().as_fd().try_clone_to_owned().unwrap());
    frame(&mut socket, &key.verifying_key().to_bytes()).unwrap();
    while let Ok((nonce, q)) = read::<([u8; 32], Query)>(&mut socket) {
        let answer = match q {
            Query::Head => ledger.current().map(|h| Answer::Head(Some(h))),
            Query::Outcome(id) => ledger.outcome(id).map(Answer::Head),
            Query::Receipt(id) => ledger.outcome(id).and_then(|h| {
                let head = h.ok_or(Fault::Denied)?;
                Ok(Answer::Receipt(SignedReceipt {
                    head,
                    signature: key.sign(&bytes(head)).to_bytes().to_vec(),
                }))
            }),
            Query::New => ledger.ensure_unused().map(|_| Answer::Ok),
            Query::Bind(h) => ledger.bind_new(h).map(|_| Answer::Ok),
            Query::Decide(prev, next) => ledger.decide(prev, next).map(|_| Answer::Ok),
        }
        .unwrap_or_else(|e| match e {
            Fault::Conflict => Answer::Conflict,
            _ => Answer::Refused,
        });
        let signature = key
            .sign(&response_bytes(&nonce, &q, &answer))
            .to_bytes()
            .to_vec();
        if frame(&mut socket, &(answer, signature)).is_err() {
            break;
        }
    }
}
pub struct Independent {
    child: Option<Child>,
    socket: RefCell<UnixStream>,
    pub public: VerifyingKey,
}
impl Independent {
    pub fn connect(path: &Path, sector: u32, create: bool) -> Self {
        let (mut socket, child_socket) = UnixStream::pair().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "processus_autorite"])
            .env("NOVA_TEST_LEDGER", path)
            .env("NOVA_TEST_CREER", if create { "1" } else { "0" })
            .env("NOVA_TEST_SECTOR", sector.to_string())
            .stdin(Stdio::from(std::os::fd::OwnedFd::from(child_socket)))
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let public = VerifyingKey::from_bytes(&read(&mut socket).unwrap()).unwrap();
        Self {
            child: Some(child),
            socket: RefCell::new(socket),
            public,
        }
    }
    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            child.kill().unwrap();
            child.wait().unwrap();
        }
    }
    fn rpc(&self, q: Query) -> Result<Answer> {
        let mut nonce = [0; 32];
        File::open("/dev/urandom")
            .and_then(|mut f| f.read_exact(&mut nonce))
            .map_err(|_| Fault::Unavailable)?;
        let mut socket = self.socket.borrow_mut();
        let transport_error = || {
            if matches!(q, Query::Decide(..) | Query::Bind(_)) {
                Fault::Uncertain
            } else {
                Fault::Unavailable
            }
        };
        frame(&mut socket, &(nonce, &q)).map_err(|_| transport_error())?;
        let (answer, raw): (Answer, Vec<u8>) = read(&mut socket).map_err(|_| transport_error())?;
        let signature = Signature::from_slice(&raw).map_err(|_| Fault::Denied)?;
        self.public
            .verify_strict(&response_bytes(&nonce, &q, &answer), &signature)
            .map_err(|_| Fault::Denied)?;
        match answer {
            Answer::Conflict => Err(Fault::Conflict),
            Answer::Refused => Err(Fault::Denied),
            a => Ok(a),
        }
    }
    fn unit(&self, q: Query) -> Result<()> {
        match self.rpc(q)? {
            Answer::Ok => Ok(()),
            _ => Err(Fault::Denied),
        }
    }
    pub fn receipt(&self, id: Id) -> SignedReceipt {
        match self.rpc(Query::Receipt(id)).unwrap() {
            Answer::Receipt(r) if r.head.request == id && r.verify(&self.public) => r,
            _ => panic!("reçu invalide"),
        }
    }
}
impl Drop for Independent {
    fn drop(&mut self) {
        self.stop()
    }
}
impl Authority for Independent {
    fn current(&self) -> Result<Head> {
        match self.rpc(Query::Head)? {
            Answer::Head(Some(h)) => Ok(h),
            _ => Err(Fault::Denied),
        }
    }
    fn outcome(&self, id: Id) -> Result<Option<Head>> {
        match self.rpc(Query::Outcome(id))? {
            Answer::Head(h) => Ok(h),
            _ => Err(Fault::Denied),
        }
    }
    fn decide(&mut self, old: Head, new: Head) -> Result<()> {
        self.unit(Query::Decide(old, new))
    }
}
impl Provisioner for Independent {
    fn ensure_unused(&self) -> Result<()> {
        self.unit(Query::New)
    }
    fn bind_new(&mut self, h: Head) -> Result<()> {
        self.unit(Query::Bind(h))
    }
}
