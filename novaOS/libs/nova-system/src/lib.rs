//! Bibliothèque utilisateur pour l’ABI native NovaOS v1.
#![no_std]

#[cfg(not(any(
    test,
    all(
        target_os = "none",
        any(target_arch = "aarch64", target_arch = "x86_64")
    )
)))]
compile_error!("Compiler pour la cible nue NovaOS AArch64 ou x86_64.");

#[allow(dead_code)]
#[path = "../../../abi/native.rs"]
mod abi;
#[cfg(target_os = "none")]
#[path = "../../../user/gate.rs"]
mod gate;
#[cfg(not(target_os = "none"))]
mod gate {
    pub fn call(_: u64, _: u64, _: u64, _: u64) -> super::abi::Reply {
        panic!("porte native NovaOS appelée sur un hôte")
    }
}

pub mod files;
pub mod ipc;
pub mod process;
pub mod sync;
pub mod thread;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Error(pub u64);
pub type Result<T> = core::result::Result<T, Error>;

fn value(r: abi::Reply) -> Result<u64> {
    match r.status {
        abi::OK => Ok(r.value),
        status => Err(Error(status)),
    }
}

fn call(id: u64, a: u64, b: u64, c: u64) -> Result<u64> {
    value(gate::call(id, a, b, c))
}

/// Contrats binaires pour les sondes et les bibliothèques de plus haut niveau.
pub mod raw {
    pub use super::abi::*;

    /// # Safety
    /// L’appelant garantit validité, droits et exclusivité des plages empruntées
    /// par la porte pendant toute sa durée, même si le thread est suspendu.
    pub unsafe fn call(id: u64, a: u64, b: u64, c: u64) -> Reply {
        super::gate::call(id, a, b, c)
    }
}

pub mod clock {
    /// Secondes UTC depuis 1970, selon la RTC admise par le noyau.
    pub fn utc_seconds() -> crate::Result<u64> {
        crate::call(crate::abi::CLOCK_UTC, 0, 0, 0)
    }
}

pub mod memory {
    pub fn heap_end() -> crate::Result<usize> {
        crate::call(crate::abi::HEAP_RESIZE, 0, 0, 0).map(|end| end as usize)
    }

    /// Déplace la borne si elle est encore égale à `expected`.
    ///
    /// # Safety
    /// Réserver cet appel à l’allocateur qui possède le tas du processus.
    /// Aucun prêt ni accès concurrent ne doit survivre à un retrait de pages.
    pub unsafe fn resize_heap(expected: usize, end: usize) -> crate::Result<usize> {
        crate::call(crate::abi::HEAP_RESIZE, expected as u64, end as u64, 0)
            .map(|value| value as usize)
    }
}

// Mêmes descripteurs, mêmes règles de validation des trames que les clients livrés.
#[allow(dead_code)]
#[path = "../../../user/storage/fileapi.rs"]
mod file_client;
#[allow(dead_code)]
#[path = "../../../abi/files.rs"]
mod protocol;
#[allow(dead_code)]
mod runtime {
    pub use crate::{abi::*, gate::call};
    pub type Result<T> = core::result::Result<T, u64>;
    pub const TIMEOUT: u64 = 250;
    pub use crate::client_io::*;

    pub fn value(reply: Reply) -> Result<u64> {
        crate::value(reply).map_err(|status| status.0)
    }
    // Constructeurs historiques privés de Client. La façade publique du SDK
    // exige deux endpoints et utilise exclusivement scoped_ports.
    pub fn channels() -> Result<(u64, u64)> {
        let tx = value(call(IPC_BOOTSTRAP, 0, 0, 0))?;
        // Sûreté : le contrat de lancement du service fixe la page DATA à 0x401000.
        let rx = unsafe { (0x401000 as *const u64).read_volatile() };
        Ok((tx, rx))
    }
    pub(crate) use crate::wire_bulk as bulk;
}
#[allow(dead_code)]
#[path = "../../../user/storage/client_io.rs"]
mod client_io;
#[allow(dead_code)]
#[path = "../../../abi/bulk_call.rs"]
mod wire_bulk;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_tls_index_outside_page_before_syscall() {
        assert_eq!(thread::tls_set(512, 1), Err(Error(abi::BAD_ARGUMENT)));
        assert_eq!(thread::tls_get(usize::MAX), Err(Error(abi::BAD_ARGUMENT)));
    }
    #[test]
    fn refuses_oversized_ipc_payload_before_syscall() {
        assert_eq!(
            client_io::send_for(1, 17, &[0; 129], 3),
            Err(abi::BAD_ARGUMENT)
        );
    }
    #[test]
    fn errors_preserve_unknown_status_and_discard_value() {
        assert_eq!(
            value(abi::Reply {
                status: 72,
                value: 17
            }),
            Err(Error(72))
        );
        assert_eq!(
            value(abi::Reply {
                status: 0,
                value: 17
            }),
            Ok(17)
        );
    }
}
