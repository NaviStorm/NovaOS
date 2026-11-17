#![no_std]
#![no_main]
extern crate alloc;
use alloc::{format, vec::Vec};
use nova_cli::runtime as rt;
#[path = "../../components/realpath/src/options.rs"]
mod options;
#[path = "../../components/realpath/src/path.rs"]
mod path;
use path::{Error, Namespace, Relative};
struct Fixture;
impl Namespace for Fixture {
    fn cwd(&mut self) -> Result<Vec<u8>, Error> {
        Ok(b"/".to_vec())
    }
    fn link(&mut self, p: &[u8]) -> Result<Option<Vec<u8>>, Error> {
        match p {
            b"/alias" => Ok(Some(b"tree/leaf".to_vec())),
            b"/absolute" => Ok(Some(b"/tree/leaf".to_vec())),
            b"/cycle" => Ok(Some(b"cycle".to_vec())),
            b"/broken" => Ok(Some(b"missing".to_vec())),
            _ => self.check(p, false).map(|_| None),
        }
    }
    fn check(&mut self, p: &[u8], dir: bool) -> Result<(), Error> {
        match p {
            b"/" | b"/alias" | b"/absolute" | b"/tree" | b"/tree/leaf" | b"/secret" => Ok(()),
            b"/cycle" => Err(Error::Loop),
            b"/tree/file" if !dir => Ok(()),
            b"/tree/file" => Err(Error::NotDirectory),
            b"/secret/child" => Err(Error::System(13)),
            _ => Err(Error::Missing),
        }
    }
}
fn log(text: &str) {
    assert_eq!(
        rt::call(rt::DEBUG_WRITE, text.as_ptr() as u64, text.len() as u64, 0).status,
        0
    )
}
fn number(data: &mut &[u8]) -> usize {
    let (n, rest) = data.split_at(4);
    *data = rest;
    u32::from_le_bytes(n.try_into().unwrap()) as usize
}
fn blob<'a>(data: &mut &'a [u8]) -> &'a [u8] {
    let n = number(data);
    let (v, rest) = data.split_at(n);
    *data = rest;
    v
}
fn vectors() {
    let mut data = include_bytes!(env!("REALPATH_VECTEURS")).as_slice();
    let count = number(&mut data);
    for i in 0..count {
        let argc = number(&mut data);
        let names: Vec<_> = (0..argc).map(|_| blob(&mut data)).collect();
        let Ok(options::Action::Paths(opt)) = options::parse(&names, false) else {
            rt::exit(2)
        };
        let mut fs = Fixture;
        let rel = Relative::new(&mut fs, &opt).unwrap();
        let (mut out, mut err, mut status) = (Vec::new(), Vec::new(), 0);
        for name in &opt.names {
            match path::resolve(&mut fs, name, &opt) {
                Ok(p) => {
                    out.extend(rel.display(p));
                    out.push(if opt.zero { 0 } else { 10 })
                }
                Err(e) => {
                    status = 1;
                    let reason = match e {
                        Error::Missing => "No such file or directory",
                        Error::Loop => "Too many levels of symbolic links",
                        Error::System(13) => "Permission denied",
                        Error::NotDirectory => "Not a directory",
                        _ => "unexpected",
                    };
                    err.extend(b"realpath: ");
                    if name.is_empty() {
                        err.extend(b"''")
                    } else {
                        err.extend_from_slice(name)
                    }
                    err.extend(format!(": {reason}\n").bytes())
                }
            }
        }
        let rc = number(&mut data);
        let stdout = blob(&mut data);
        let stderr = blob(&mut data);
        if (rc, stdout, stderr) != (status, &out[..], &err[..]) {
            log(&format!("REALPATH ECART {i}"));
            rt::exit(1)
        }
    }
    assert!(data.is_empty());
    log(&format!("REALPATH MATRICE {count} OK"))
}
fn bench(n: usize, name: &[u8], reps: usize, expected: &[u8], heap_base: u64) {
    let mut args = alloc::vec![b"-m".as_slice()];
    args.extend(core::iter::repeat_n(name, n));
    log(&format!("REALPATH DEBUT {n} {} {reps} 0", name.len()));
    for pass in 0..4 {
        for _ in 0..reps {
            let Ok(options::Action::Paths(opt)) =
                options::parse(core::hint::black_box(&args), false)
            else {
                rt::exit(2)
            };
            for arg in &opt.names {
                let p = path::resolve(&mut Fixture, arg, &opt).unwrap();
                assert_eq!(core::hint::black_box(&p), expected);
            }
        }
        let end = rt::value(rt::call(rt::HEAP_RESIZE, 0, 0, 0)).unwrap();
        log(&format!("REALPATH FIN {n} {pass} {}", end - heap_base));
    }
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    let heap_base = rt::value(rt::call(rt::HEAP_RESIZE, 0, 0, 0)).unwrap();
    vectors();
    bench(1, b"/tree/file", 500, b"/tree/file", heap_base);
    bench(32, b"/alias/../file", 100, b"/tree/file", heap_base);
    bench(4, b"/a/b/c/d/e/f/g/h", 250, b"/a/b/c/d/e/f/g/h", heap_base);
    rt::exit(0)
}
