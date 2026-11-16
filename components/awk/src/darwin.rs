use awk_rs::{
    Error, Result,
    host::{Host, Input, Output},
};
use std::{
    fs::{File, OpenOptions},
    io::{self, BufRead, BufReader, Read, Write},
    process::{Child, Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

pub struct Darwin;

fn shell(command: &str) -> Command {
    let mut process = Command::new("/bin/sh");
    process.args(["-c", command]);
    process
}

struct Stream<T> {
    handle: Option<T>,
    child: Option<Child>,
}

impl<T> Stream<T> {
    fn finish(&mut self) -> io::Result<i32> {
        self.handle.take();
        self.child
            .take()
            .map_or(Ok(0), |mut child| Ok(child.wait()?.code().unwrap_or(-1)))
    }
}

impl<T> Drop for Stream<T> {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}
impl<T: Read> Read for Stream<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.handle.as_mut().unwrap().read(buf)
    }
}
impl<T: BufRead> BufRead for Stream<T> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.handle.as_mut().unwrap().fill_buf()
    }
    fn consume(&mut self, size: usize) {
        self.handle.as_mut().unwrap().consume(size);
    }
}
impl<T: BufRead> Input for Stream<T> {
    fn close(mut self: Box<Self>) -> Result<i32> {
        Ok(self.finish()?)
    }
}
impl<T: Write> Write for Stream<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.handle.as_mut().unwrap().write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.handle.as_mut().unwrap().flush()
    }
}
impl<T: Write> Output for Stream<T> {
    fn close(mut self: Box<Self>) -> Result<i32> {
        self.flush()?;
        Ok(self.finish()?)
    }
}

impl Host for Darwin {
    fn input(&self, name: &str, pipe: bool) -> Result<Box<dyn Input>> {
        if pipe {
            let mut child = shell(name).stdout(Stdio::piped()).spawn()?;
            let reader = BufReader::new(child.stdout.take().unwrap());
            Ok(Box::new(Stream {
                handle: Some(reader),
                child: Some(child),
            }))
        } else {
            Ok(Box::new(Stream {
                handle: Some(BufReader::new(File::open(name)?)),
                child: None,
            }))
        }
    }
    fn output(&self, name: &str, append: bool, pipe: bool) -> Result<Box<dyn Output>> {
        if pipe {
            let mut child = shell(name).stdin(Stdio::piped()).spawn()?;
            Ok(Box::new(Stream {
                handle: child.stdin.take(),
                child: Some(child),
            }))
        } else {
            let file = OpenOptions::new()
                .write(true)
                .create(true)
                .append(append)
                .truncate(!append)
                .open(name)?;
            Ok(Box::new(Stream {
                handle: Some(file),
                child: None,
            }))
        }
    }
    fn system(&self, command: &str) -> Result<i32> {
        Ok(shell(command).status()?.code().unwrap_or(-1))
    }
    fn seconds(&self) -> Result<u64> {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|time| time.as_secs())
            .map_err(|e| Error::runtime(e.to_string()))
    }
    fn pid(&self) -> Option<u32> {
        Some(std::process::id())
    }
}
