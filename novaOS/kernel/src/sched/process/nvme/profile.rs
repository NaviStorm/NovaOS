//! Sélection fermée des images et des droits du laboratoire. Les modes de
//! récupération et de vérification disposent uniquement de la lecture.
pub(in crate::sched::process) const INIT: u64 = 1000;
pub(in crate::sched::process) const UPDATE: u64 = 1001;
pub(in crate::sched::process) const RECOVER: u64 = 1002;
pub(in crate::sched::process) const REVOKE_BEFORE: u64 = 1003;
pub(in crate::sched::process) const REVOKE_AFTER: u64 = 1004;
pub(in crate::sched::process) const TIMEOUT_FLUSH: u64 = 1005;
pub(in crate::sched::process) const REUSE: u64 = 1006;
#[cfg(feature="net-q35-duo")]
pub(in crate::sched::process) const MEASURE: u64 = 1007;
pub(in crate::sched::process) const RIGHTS: u64 = 1010;
pub(in crate::sched::process) const REVOKE_WRITE: u64 = 1011;
pub(in crate::sched::process) const REVOKE_FLUSH: u64 = 1012;
pub(in crate::sched::process) const TIMEOUT: u64 = 1013;
pub(in crate::sched::process) const DRIVER_DEATH: u64 = 1014;
pub(in crate::sched::process) const CLIENT_DEATH: u64 = 1015;
pub(in crate::sched::process) const DMA_FAULT: u64 = 1016;
pub(in crate::sched::process) const QI_ERROR: u64 = 1017;
pub(in crate::sched::process) const QI_DELAY: u64 = 1018;
pub(in crate::sched::process) const RACE: u64 = 1019;
const CUT_BASE: u64 = 1020;

pub(in crate::sched::process) fn parse(name: &[u8]) -> Option<u64> {
    Some(match name.strip_prefix(b"nvme-rw-")? {
        b"init" => INIT,
        b"update" => UPDATE,
        b"recover" => RECOVER,
        b"revoke-before" => REVOKE_BEFORE,
        b"revoke-after" => REVOKE_AFTER,
        b"timeout-flush" => TIMEOUT_FLUSH,
        b"reuse" => REUSE,
        b"rights" => RIGHTS,
        b"revoke-write" => REVOKE_WRITE,
        b"revoke-flush" => REVOKE_FLUSH,
        b"timeout" => TIMEOUT,
        b"driver-death" => DRIVER_DEATH,
        b"client-death" => CLIENT_DEATH,
        b"dma-fault" => DMA_FAULT,
        b"qi-error" => QI_ERROR,
        b"qi-delay" => QI_DELAY,
        b"race" => RACE,
        b"cut-before-write" => CUT_BASE + 1,
        b"cut-during-write" => CUT_BASE + 2,
        b"cut-after-write" => CUT_BASE + 3,
        b"cut-during-f1" => CUT_BASE + 4,
        b"cut-after-f1" => CUT_BASE + 5,
        b"cut-during-publish" => CUT_BASE + 6,
        b"cut-during-f2" => CUT_BASE + 7,
        b"cut-after-f2" => CUT_BASE + 8,
        _ => return None,
    })
}

pub(in crate::sched::process) fn writable(mode: u64) -> bool {
    mode >= INIT && !matches!(mode, RECOVER | RIGHTS)
}
pub(in crate::sched::process) fn file_mode(mode: u64) -> u64 {
    match mode {
        INIT => INIT,
        RECOVER | RIGHTS => RECOVER,
        _ => UPDATE,
    }
}
pub(in crate::sched::process) fn cut(mode: u64) -> u64 {
    mode.saturating_sub(CUT_BASE)
}
