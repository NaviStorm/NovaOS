//! Réponses de Simple Text Output exercées hors firmware sur les deux ISA.
#![allow(unexpected_cfgs)]
extern crate alloc;

#[allow(dead_code)]
#[path = "../../diagnostic-5e/phase5h.rs"]
mod phase5h;

use core::ptr;
use r_efi::{efi, protocols::simple_text_output::Protocol};

extern "efiapi" fn indisponible(_: *mut Protocol, _: *mut u16) -> efi::Status {
    efi::Status::DEVICE_ERROR
}

extern "efiapi" fn sortie(_: *mut Protocol, text: *mut u16) -> efi::Status {
    // Le témoin attend exactement 125 'x', CR, LF et le terminateur dans les 128 mots.
    let attendu = core::iter::repeat_n(u16::from(b'x'), 125).chain([13, 10, 0]);
    if unsafe { core::slice::from_raw_parts(text, 128) }
        .iter()
        .copied()
        .eq(attendu)
    {
        efi::Status::SUCCESS
    } else {
        efi::Status::DEVICE_ERROR
    }
}

extern "efiapi" fn bool_service(_: *mut Protocol, _: efi::Boolean) -> efi::Status {
    efi::Status::UNSUPPORTED
}
extern "efiapi" fn query(_: *mut Protocol, _: usize, _: *mut usize, _: *mut usize) -> efi::Status {
    efi::Status::UNSUPPORTED
}
extern "efiapi" fn index(_: *mut Protocol, _: usize) -> efi::Status {
    efi::Status::UNSUPPORTED
}
extern "efiapi" fn clear(_: *mut Protocol) -> efi::Status {
    efi::Status::UNSUPPORTED
}
extern "efiapi" fn position(_: *mut Protocol, _: usize, _: usize) -> efi::Status {
    efi::Status::UNSUPPORTED
}

#[test]
fn console_absente_erreur_de_sortie_et_limite_utf16() {
    // SystemTable ne contient que des entiers et des pointeurs bruts. Seul ConOut est lu.
    let mut st: efi::SystemTable = unsafe { core::mem::zeroed() };
    assert_eq!(phase5h::display(&st, "verdict"), Err(17));
    let mut con = Protocol {
        reset: bool_service,
        output_string: indisponible,
        test_string: indisponible,
        query_mode: query,
        set_mode: index,
        set_attribute: index,
        clear_screen: clear,
        set_cursor_position: position,
        enable_cursor: bool_service,
        mode: ptr::null_mut(),
    };
    st.con_out = &raw mut con;
    assert_eq!(phase5h::display(&st, "verdict"), Err(17));
    con.output_string = sortie;
    st.con_out = &raw mut con;
    assert_eq!(phase5h::display(&st, &"x".repeat(125)), Ok(()));
    assert_eq!(phase5h::display(&st, &"x".repeat(126)), Err(17));
    assert_eq!(phase5h::display(&st, "é"), Err(17));
}
