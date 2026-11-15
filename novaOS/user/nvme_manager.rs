//! Sélection Ring 3 du pilote intégré au profil NVMe. Le superviseur désigne
//! séparément le modèle observé et l’autorisation du manifeste après reprise PCI.
#![no_std]
#![no_main]
#[allow(dead_code)]
mod device_lifecycle;
#[allow(dead_code)]
mod nvme_protocol;
#[allow(dead_code)]
#[path = "storage/runtime.rs"]
mod runtime;
use device_lifecycle::{Authorization, Device, Driver, Instance};
use nvme_protocol::{DRIVER_IMAGE, DRIVER_INTENTS_RO};
use runtime::config;

fn select() -> Option<()> {
    let location = u32::try_from(config(0)).ok()?;
    let mut controller = Device::observed(
        location,
        u16::try_from(config(1)).ok()?,
        u16::try_from(config(2)).ok()?,
    );
    if config(3) != 0x01_08_02 {
        return None;
    }
    let approved = Authorization {
        instance: Instance {
            location: u32::try_from(config(4)).ok()?,
            generation: 1,
        },
        image: u32::try_from(config(5)).ok()?,
        rights: u32::try_from(config(6)).ok()?,
    };
    let catalog = [Driver {
        vendor: 0x1b36,
        product: 0x0010,
        image: DRIVER_IMAGE,
        required: DRIVER_INTENTS_RO,
    }];
    controller.start(&catalog, Some(approved)).ok()?;
    Some(())
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    runtime::exit(if select().is_some() {
        0
    } else {
        runtime::DENIED
    })
}
