use crate::{Result, abi, call};

pub fn id() -> Result<u64> {
    call(abi::PROCESS_ID, 0, 0, 0)
}

pub fn exit(code: u64) -> ! {
    let _ = call(abi::PROCESS_EXIT, code, 0, 0);
    panic!("PROCESS_EXIT a rendu la main")
}

/// Résultat d’un processus couvert par le droit d’attente du lanceur.
pub fn wait(key: u64) -> Result<abi::WaitInfo> {
    let mut result = abi::WaitInfo {
        version: 0,
        size: 0,
        process: 0,
        kind: 0,
        code: 0,
    };
    call(
        abi::PROCESS_WAIT,
        key,
        (&raw mut result) as u64,
        abi::WAIT_SIZE as u64,
    )?;
    Ok(result)
}
