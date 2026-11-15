//! Profil CACHE de qualification. L'acteur 11 vivant occupe déjà son unique slot.
//! Un START valide atteint le contrôle de doublon. Une trame invalide doit être
//! rejetée par le noyau avant cette étape, sans écrire le tampon de retour.
use crate::{Frame, p::service_caps as caps, proc, rt, syscall};

pub(super) fn occupied_actor() -> rt::Result<()> {
    let mut base: Frame = [0; 16];
    base[0] = 11;
    base[4] = 1;
    let expect = |mut frame: Frame, status| {
        let original = frame;
        if syscall(proc::START, &mut frame) == Err(status) && original == frame {
            Ok(())
        } else {
            Err(rt::REFUSED)
        }
    };
    expect(base, rt::REFUSED)?;
    for (version, mask, status) in [
        (caps::START_MASK_VERSION, 0, rt::REFUSED),
        (caps::START_MASK_VERSION, caps::PRIVATE_TMP | caps::CACHE_CLIENT, rt::REFUSED),
        (caps::START_MASK_VERSION, caps::CACHE_BACKEND, rt::DENIED),
        (caps::START_MASK_VERSION, 1 << 63, rt::DENIED),
        (0, caps::PRIVATE_TMP, rt::BAD_ARGUMENT),
        (caps::START_MASK_VERSION + 1, 0, rt::BAD_ARGUMENT),
    ] {
        let mut frame = base;
        frame[5] = version;
        frame[6] = mask;
        expect(frame, status)?;
    }
    for index in [1, 2].into_iter().chain(7..16) {
        let mut frame = base;
        frame[5] = caps::START_MASK_VERSION;
        frame[index] = 1;
        expect(frame, rt::BAD_ARGUMENT)?;
    }
    Ok(())
}
