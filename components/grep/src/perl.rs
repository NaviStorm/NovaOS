use std::ffi::{c_int, c_void};
use std::io;
use std::ptr::NonNull;

unsafe extern "C" {
    fn grep_perl_compile(
        pattern: *const u8,
        len: usize,
        utf: c_int,
        caseless: c_int,
        error: *mut c_int,
    ) -> *mut c_void;
    fn grep_perl_find(
        re: *mut c_void,
        text: *const u8,
        len: usize,
        start: usize,
        not_bol: c_int,
        span: *mut usize,
    ) -> c_int;
    fn grep_perl_error(code: c_int, out: *mut u8, len: usize) -> c_int;
    fn grep_perl_free(re: *mut c_void);
}

fn message(code: c_int) -> String {
    let mut buffer = [0; 256];
    let len = unsafe { grep_perl_error(code, buffer.as_mut_ptr(), buffer.len()) };
    if len < 0 {
        return format!("PCRE2 error {code}");
    }
    String::from_utf8_lossy(&buffer[..len as usize]).into_owned()
}

pub(crate) struct Perl(NonNull<c_void>, bool);

impl Perl {
    pub fn compile(pattern: &[u8], utf: bool, ignore_case: bool) -> Result<Self, String> {
        let mut error = 0;
        let ptr = unsafe {
            grep_perl_compile(
                pattern.as_ptr(),
                pattern.len(),
                utf.into(),
                ignore_case.into(),
                &mut error,
            )
        };
        NonNull::new(ptr)
            .map(|ptr| Self(ptr, utf))
            .ok_or_else(|| message(error))
    }

    pub fn find(&mut self, line: &[u8], offset: usize) -> io::Result<Option<(usize, usize)>> {
        let mut span = [0; 2];
        // In a UTF locale an encoding error at byte zero cannot start a line
        // match (^). PCRE2 still evaluates the absolute subject anchor (\A).
        let first_invalid = self.1
            && line.first().is_some_and(|first| !first.is_ascii())
            && line
                .utf8_chunks()
                .next()
                .is_some_and(|chunk| chunk.valid().is_empty());
        let rc = unsafe {
            grep_perl_find(
                self.0.as_ptr(),
                line.as_ptr(),
                line.len(),
                offset,
                first_invalid.into(),
                span.as_mut_ptr(),
            )
        };
        match rc {
            0 => Ok(None),
            1 => Ok(Some((span[0], span[1]))),
            error => Err(io::Error::other(message(error))),
        }
    }
}

impl Drop for Perl {
    fn drop(&mut self) {
        unsafe { grep_perl_free(self.0.as_ptr()) }
    }
}
