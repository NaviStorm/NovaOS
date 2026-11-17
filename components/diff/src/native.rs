use super::*;
use nova_cli::{directory, fileapi::Client, protocol as w, runtime as rt};
#[path = "launch/mod.rs"]
mod launch;
use launch::Launch;
fn error(path: &[u8], code: u64) -> String {
    format!(
        "{}: {} ({code})",
        txt(path),
        nova_cli::console::reason(code)
    )
}
struct Guest<'a>(&'a Launch<'a>);
impl Platform for Guest<'_> {
    fn utf8(&self) -> bool {
        self.0.utf8
    }
    fn stat(&mut self, path: &[u8], o: &Options) -> Result<Option<Meta>, String> {
        if path == b"-" {
            return Ok(Some(Meta::file()));
        }
        let mut c = self.0.client().map_err(|e| error(path, e))?;
        match c.at(w::LOOKUP_AT, 0, path, 0) {
            Ok(frame) => Ok(Some(Meta {
                kind: if frame[w::PAYLOAD + 24] == 2 {
                    Kind::Dir
                } else {
                    Kind::File
                },
                id: Some((0, w::word(&frame, w::PAYLOAD))),
                stamp: if w::length(&frame) == 73 {
                    let p = w::PAYLOAD + 37;
                    let sec = i64::from_le_bytes(frame[p..p + 8].try_into().unwrap());
                    let ns = u32::from_le_bytes(frame[p + 8..p + 12].try_into().unwrap());
                    if ns < 1_000_000_000 {
                        calendar::Date::utc(sec).stamp(ns, o.style == "context")
                    } else {
                        b"unknown".to_vec()
                    }
                } else {
                    b"unknown".to_vec()
                },
            })),
            Err(w::NOT_FOUND) => Ok(None),
            Err(e) => Err(error(path, e)),
        }
    }
    fn read(&mut self, path: &[u8]) -> Result<Vec<u8>, String> {
        self.0
            .with_input(path, |read| {
                let mut data = vec![];
                let mut buf = [0; 1024];
                loop {
                    let n = read(&mut buf)?;
                    if n == 0 {
                        return Ok(data);
                    }
                    data.try_reserve(n).map_err(|_| rt::NO_MEMORY)?;
                    data.extend(&buf[..n])
                }
            })
            .map_err(|e| error(path, e))
    }
    fn list(&mut self, path: &[u8]) -> Result<Vec<Vec<u8>>, String> {
        let mut c = self.0.client().map_err(|e| error(path, e))?;
        let dir = c
            .directory(0, path, w::DIR_RO)
            .map_err(|e| error(path, e))?;
        let mut names = vec![];
        let result = directory::visit_names(&mut c, dir, usize::MAX, |_, s| {
            if s != "." && s != ".." {
                names.push(s.as_bytes().to_vec())
            }
            Ok(())
        });
        let close = c.simple(w::DIR_CLOSE, dir);
        result.and(close.map(|_| ())).map_err(|e| error(path, e))?;
        Ok(names)
    }
    fn paginate(&mut self, title: &[u8], data: &[u8]) -> Result<Vec<u8>, String> {
        let sec = rt::value(rt::call(rt::CLOCK_UTC, 0, 0, 0)).map_err(|e| error(b"clock", e))?;
        Ok(calendar::Date::utc(sec as i64).paginate(title, data))
    }
    fn emit(&mut self, err: bool, b: &[u8]) -> Result<(), String> {
        if err {
            self.0.write_stderr(b)
        } else {
            self.0.write_stdout(b)
        }
        .map_err(|e| error(b"output", e))
    }
}
impl Launch<'_> {
    fn client(&self) -> rt::Result<Client> {
        self.file_ports().map(Client::scoped_ports)
    }
}
#[cfg(not(feature = "temoin"))]
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    let Ok(launch) = Launch::current() else {
        rt::exit(2)
    };
    let args: Vec<_> = launch.argv.iter().map(|s| s.as_bytes()).collect();
    let rc = execute(
        &mut Guest(&launch),
        &args,
        b"diff",
        launch.french,
        launch.posix_options,
        false,
    );
    rt::exit(rc.into())
}
