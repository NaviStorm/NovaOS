mod acpi;
pub(crate) mod discovery;
#[allow(dead_code)]
mod driver_resources;
mod pci_inventory;
pub(crate) mod block;
pub mod framebuffer;
#[cfg(target_arch = "x86_64")]
mod pci_ports;
#[cfg(target_arch = "aarch64")]
mod virt_dtb;
