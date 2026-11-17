pub fn native_units(mut data: &[u8], mut visit: impl FnMut(&[u8], bool)) {
    unsafe extern "C" {
        fn mbrtowc(wide: *mut i32, src: *const u8, len: usize, state: *mut u64) -> usize;
        fn iswprint(wide: i32) -> i32;
    }
    // arm/_types.h : mbstate_t est une union de 128 octets alignée sur 8.
    let mut state = [0u64; 16];
    while !data.is_empty() {
        let mut wide = 0;
        let n = unsafe { mbrtowc(&mut wide, data.as_ptr(), data.len(), state.as_mut_ptr()) };
        let valid = n > 0 && n <= data.len();
        let len = if valid {
            n
        } else {
            state.fill(0);
            1
        };
        visit(&data[..len], valid && unsafe { iswprint(wide) != 0 });
        data = &data[len..];
    }
}
