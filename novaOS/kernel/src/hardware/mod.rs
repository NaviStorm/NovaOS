mod acpi;
#[cfg(all(target_arch = "x86_64", feature = "vtd-lab"))]
pub(crate) mod vtd_lab;
#[cfg(all(target_arch = "x86_64", feature = "vtd-lab"))]
pub(crate) mod edu_dma;
#[cfg(all(target_arch = "x86_64", feature = "nvme-lab"))]
pub(crate) mod nvme;
#[cfg(feature = "materiel-lab")]
pub(crate) mod lab_irq;
pub(crate) mod discovery;
#[allow(dead_code)]
mod driver_resources;
#[allow(dead_code)]
pub(crate) mod physical_dma;
mod pci_inventory;
pub(crate) mod block;
pub mod framebuffer;
#[cfg(target_arch = "x86_64")]
mod pci_ports;
#[cfg(target_arch = "aarch64")]
mod virt_dtb;

#[cfg(all(target_arch = "x86_64", feature = "nvme-lab"))]
pub(crate) mod nvme_domain;
