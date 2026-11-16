use alloc::{
    alloc::{Layout, alloc, dealloc, realloc},
    string::String,
};
use core::{
    ffi::{CStr, c_char, c_void},
    ptr::{self, NonNull},
    slice,
};
// QSE fait passer chaque allocation par ces trois fonctions, y compris en cas d'échec.
const HEADER: usize = 16;
fn layout(n: usize) -> Option<Layout> {
    Layout::from_size_align(n.checked_add(HEADER)?, HEADER).ok()
}
#[unsafe(no_mangle)]
extern "C" fn sed_alloc(n: usize) -> *mut u8 {
    let Some(l) = layout(n) else {
        return ptr::null_mut();
    };
    unsafe {
        let p = alloc(l);
        if p.is_null() {
            p
        } else {
            p.cast::<usize>().write(n);
            p.add(HEADER)
        }
    }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn sed_resize(p: *mut u8, n: usize) -> *mut u8 {
    if p.is_null() {
        return sed_alloc(n);
    }
    let Some(l) = layout(n) else {
        return ptr::null_mut();
    };
    unsafe {
        let base = p.sub(HEADER);
        let old = layout(base.cast::<usize>().read()).unwrap();
        let next = realloc(base, old, l.size());
        if next.is_null() {
            next
        } else {
            next.cast::<usize>().write(n);
            next.add(HEADER)
        }
    }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn sed_free(p: *mut u8) {
    if !p.is_null() {
        unsafe {
            let base = p.sub(HEADER);
            dealloc(base, layout(base.cast::<usize>().read()).unwrap())
        }
    }
}
pub trait Streams {
    fn open(&mut self, path: &[u8], out: bool, persistent: bool) -> Result<usize, String>;
    fn close(&mut self, handle: usize) -> Result<(), String>;
    fn read(&mut self, handle: usize, buf: &mut [u8]) -> Result<usize, String>;
    fn write(&mut self, handle: usize, data: &[u8]) -> Result<(), String>;
    fn shell(&mut self, command: &[u8]) -> Result<alloc::vec::Vec<u8>, String>;
    fn filename(&self) -> &[u8];
}
struct Context<'a> {
    io: Option<&'a mut dyn Streams>,
    error: Option<String>,
}
unsafe extern "C" {
    fn sed_compile(
        script: *const u8,
        len: usize,
        flags: i32,
        width: usize,
        ctx: *mut c_void,
    ) -> *mut c_void;
    fn sed_execute(s: *mut c_void, ctx: *mut c_void) -> i32;
    fn qse_sed_close(s: *mut c_void);
}
#[unsafe(no_mangle)]
unsafe extern "C" fn sed_error(ctx: &mut Context<'_>, msg: *const c_char) {
    ctx.error.get_or_insert_with(|| {
        unsafe { CStr::from_ptr(msg) }
            .to_string_lossy()
            .into_owned()
    });
}
#[unsafe(no_mangle)]
unsafe extern "C" fn sed_io(
    ctx: &mut Context<'_>,
    out: i32,
    op: i32,
    path: *const c_char,
    handle: &mut usize,
    data: *mut u8,
    n: usize,
) -> isize {
    let io = ctx.io.as_deref_mut().expect("execution context");
    let result = match op {
        0 if path.is_null() => {
            *handle = 0;
            Ok(0)
        }
        0 => io
            .open(unsafe { CStr::from_ptr(path) }.to_bytes(), out != 0, n != 0)
            .map(|h| {
                *handle = h;
                0
            }),
        1 if *handle == 0 => Ok(0),
        1 => io.close(*handle).map(|()| 0),
        2 => {
            unsafe { data.write_bytes(0, n) };
            io.read(*handle, unsafe { slice::from_raw_parts_mut(data, n) })
        }
        3 => io
            .write(*handle, unsafe { slice::from_raw_parts(data, n) })
            .map(|()| n),
        _ => Err("invalid stream request".into()),
    };
    match result {
        Ok(n) => n as isize,
        Err(e) => {
            if op != 0 || out != 0 {
                ctx.error = Some(e);
            }
            -1
        }
    }
}
pub struct Program(NonNull<c_void>);
impl Program {
    pub fn compile(script: &[u8], flags: i32, width: usize) -> Result<Self, String> {
        let mut ctx = Context {
            io: None,
            error: None,
        };
        NonNull::new(unsafe {
            sed_compile(
                script.as_ptr(),
                script.len(),
                flags,
                width,
                (&raw mut ctx).cast(),
            )
        })
        .map(Self)
        .ok_or_else(|| ctx.error.unwrap_or_else(|| "compilation failed".into()))
    }
    pub fn run(&mut self, io: &mut dyn Streams) -> Result<Option<i32>, String> {
        let mut ctx = Context {
            io: Some(io),
            error: None,
        };
        let code = unsafe { sed_execute(self.0.as_ptr(), (&raw mut ctx).cast()) };
        if let Some(e) = ctx.error {
            return Err(e);
        }
        match code {
            -2 => Err("execution failed".into()),
            -1 => Ok(None),
            n => Ok(Some(n)),
        }
    }
}
impl Drop for Program {
    fn drop(&mut self) {
        unsafe { qse_sed_close(self.0.as_ptr()) }
    }
}

unsafe fn export(
    ctx: &mut Context<'_>,
    result: Result<alloc::vec::Vec<u8>, String>,
    out: *mut *mut u8,
) -> isize {
    match result {
        Ok(bytes) => {
            let p = sed_alloc(bytes.len());
            if p.is_null() {
                ctx.error = Some("insufficient memory".into());
                return -1;
            }
            unsafe {
                ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
                out.write(p);
            }
            bytes.len() as isize
        }
        Err(e) => {
            ctx.error = Some(e);
            -1
        }
    }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn sed_system(
    ctx: &mut Context<'_>,
    p: *const u8,
    n: usize,
    out: *mut *mut u8,
) -> isize {
    let bytes = unsafe { slice::from_raw_parts(p, n) };
    let result = ctx.io.as_deref_mut().unwrap().shell(bytes);
    unsafe { export(ctx, result, out) }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn sed_filename(ctx: &mut Context<'_>, out: *mut *mut u8) -> isize {
    let bytes = ctx.io.as_ref().unwrap().filename().to_vec();
    unsafe { export(ctx, Ok(bytes), out) }
}
