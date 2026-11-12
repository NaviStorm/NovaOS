#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]

extern crate alloc;

#[path = "../../abi/native.rs"]
mod abi;
mod arch;
mod boot;
mod console;
pub mod hal;
mod hardware;
mod heap;
mod input;
mod memory;
mod sched;
#[allow(dead_code)]
#[path = "../../abi/terminal.rs"]
mod terminal;

use core::panic::PanicInfo;
use hal::log;
use hardware::framebuffer;

const BANNER: &str = concat!("NovaOS v", env!("CARGO_PKG_VERSION"));

// Sûreté : seul symbole d’entrée, désigné par ENTRY dans le script de liaison.
#[unsafe(no_mangle)]
pub extern "C" fn nova_start() -> ! {
    hal::init();
    let info = boot::memory().unwrap_or_else(|reason| panic!("{reason}"));
    // Sûreté : unique appel au point d’entrée Limine, sans AP ni allocateur antérieur.
    let mut memory = unsafe { memory::Memory::init(info) }
        .unwrap_or_else(|error| panic!("Initialisation mémoire : {error:?}"));
    log(format_args!("{BANNER}"));
    let scenario = boot::scenario();
    if scenario.starts_with(b"mem-") {
        memory
            .test(scenario)
            .unwrap_or_else(|error| panic!("Autotest mémoire : {error:?}"));
    }
    if matches!(scenario, b"abi-test" | b"ipc-test") {
        memory.report("init", memory::Resident::default());
    }
    let kernel_space = memory.space_id();
    heap::install(memory);
    if scenario.starts_with(b"heap-") {
        heap::test(scenario);
    }
    sched::start(main_thread, kernel_space)
}

fn main_thread() {
    let (args, screen) = boot::handoff();
    // Sûreté : CPU0 exécute déjà sa pile privée et ses propres tables, GDT/TSS
    // ou VBAR. Les réponses ont été copiées ; aucun lecteur Limine ne subsiste.
    match unsafe { memory::reclaim_loader(screen.as_ref().ok().map(framebuffer::Screen::span)) } {
        Ok(()) => (),
        Err(memory::Error::Exhausted) => log(format_args!(
            "Carte mémoire conservée dans les cadres Limine."
        )),
        Err(e) => panic!("Restitution du chargeur : {e:?}"),
    }
    let scenario = args.as_slice();
    #[cfg(all(target_arch = "aarch64", feature = "gicv3-lab"))]
    if cfg!(feature = "dma-sequentiel") {
        #[cfg(feature = "ancres-p9-lab")]
        if scenario == b"ancres-p9-lab" {
            let r = hardware::block::qualification::ancres::executer();
            log(format_args!("P9LAB fin {r:?}"));
            hal::cpu::halt()
        }
        #[cfg(feature = "installation-gpt")]
        if scenario == b"installer-gpt" {
            let r = hardware::block::qualification::installer();
            log(format_args!("INSTALLATION bilan {r:?}"));
            hal::cpu::halt()
        }
        #[cfg(feature = "systeme-gpt")]
        if scenario == b"systeme-gpt" {
            let r = hardware::block::qualification::systeme();
            log(format_args!("SYSTEME bilan {r:?}"));
            hal::cpu::halt()
        }
        #[cfg(feature = "dma-sequentiel")]
        if matches!(scenario, b"dma-sequentiel" | b"dma-sequentiel-reemploi") {
            if scenario == b"dma-sequentiel-reemploi" {
                heap::with_pages(memory::Memory::verify_firmware_reuse)
                    .expect("réemploi des cadres EFI");
            }
            let resultat = hardware::block::qualification::executer();
            hal::log(format_args!("CIBLE bilan {resultat:?}"));
        } else {
            hal::log(format_args!("CIBLE mode de demarrage refuse"));
        }
        hal::cpu::halt()
    }
    if scenario.starts_with(b"sched-") {
        sched::test(scenario);
    }
    if scenario == b"abi-test" {
        sched::process::abi_test();
    }
    if scenario.starts_with(b"ipc-") {
        sched::process::ipc::run(scenario);
    }
    if let Some(mode) = match scenario {
        b"namespace-f1-inflight" => Some(39),
        b"namespace-f2-inflight" => Some(40),
        b"namespace-disconnect" => Some(38),
        b"namespace-depth" => Some(36),
        b"namespace-inspect" => Some(37),
        b"cli" => Some(41),
        b"cli-test" => Some(42),
        b"namespace-write" => Some(29),
        b"namespace-read" => Some(30),
        b"namespace-rename" => Some(31),
        b"namespace-f1-before" => Some(32),
        b"namespace-f1-after" => Some(33),
        b"namespace-f2-before" => Some(34),
        b"namespace-f2-after" => Some(35),
        b"files-recover" => Some(13),
        b"files-f1-before" => Some(14),
        b"files-f1-inflight" => Some(15),
        b"files-f1-after" => Some(16),
        b"files-f2-before" => Some(17),
        b"files-f2-inflight" => Some(18),
        b"files-f2-after" => Some(19),
        b"files-data" => Some(20),
        b"files-meta" => Some(21),
        b"files-close" => Some(22),
        b"files-remount" => Some(23),
        b"files-inject" => Some(24),
        b"files-info" => Some(26),
        b"files-abandon" => Some(27),
        b"files-restart" => Some(28),
        b"files-perf" => Some(25),
        b"files-write" => Some(11),
        b"files-read" => Some(12),
        b"storage-live" => Some(3),
        b"storage-write" => Some(4),
        b"storage-read" => Some(5),
        b"storage-perf" => Some(6),
        b"storage-page-write" => Some(9),
        b"storage-page-read" => Some(10),
        b"storage-eio" => Some(8),
        _ => None,
    } {
        let ok = if mode == 28 {
            !sched::process::storage::volume(mode) && sched::process::storage::volume(13)
        } else if matches!(mode, 25 | 36) {
            sched::process::storage::file_perf(mode)
        } else {
            sched::process::storage::volume(mode)
        };
        hal::log(format_args!("BLOC session terminee ok={ok}"));
        if mode != 3 {
            hal::cpu::halt()
        }
    }
    if scenario == b"dma-fence" {
        let result = hardware::block::probe_protection();
        hal::log(format_args!("DMA bilan {result:?}"));
        hal::cpu::halt()
    }
    if matches!(scenario, b"dma-user" | b"dma-flight") {
        sched::process::storage::qualify_transport(scenario == b"dma-flight");
    }
    #[cfg(all(target_arch = "aarch64", feature = "gicv3-lab"))]
    if scenario == b"dma-reemploi" {
        heap::with_pages(memory::Memory::verify_firmware_reuse).expect("réemploi des cadres EFI");
        hardware::block::probe();
    }
    if scenario == b"dma-qual" {
        hardware::block::probe();
    }
    if scenario == b"storage-test" {
        sched::process::storage::test();
    }
    if scenario == b"elf-test" {
        sched::process::storage::elf_test();
        hal::cpu::halt()
    }
    if scenario == b"proc-lifecycle" {
        sched::process::ipc::lifecycle::test();
        hal::cpu::halt()
    }
    if scenario == b"proc-test" {
        sched::process::test();
    }
    if let Err(reason) = interactive(scenario, screen) {
        hal::interrupts::mask();
        log(format_args!("{reason}"));
        hal::cpu::halt();
    }
}

fn interactive(
    scenario: &[u8],
    screen: Result<framebuffer::Screen, &'static str>,
) -> Result<(), &'static str> {
    let terminal = (|| {
        let (screen, bottom) = framebuffer::show_banner(screen?, BANNER)?;
        log(format_args!("Affichage prêt."));
        Ok((console::Console::new(screen, bottom)?, hal::keyboard()?))
    })();
    if let Err(reason) = terminal {
        if scenario.is_empty() {
            // Le service de stockage démarre également sur la console série seule.
            log(format_args!("{reason}"));
            sched::process::storage::before_halt();
            hal::cpu::halt();
        }
        return Err(reason);
    }
    let (mut console, keyboard) = terminal?;
    match scenario {
        b"" => {
            sched::process::boot_demo(false);
            sched::process::storage::boot();
        }
        b"proc-boot-fault" => sched::process::boot_demo(true),
        _ => {}
    }
    let Some(mut next_char) = keyboard else {
        return Ok(());
    };
    loop {
        let _line = input::read_line(&mut next_char, &mut console)?;
    }
}

#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    hal::interrupts::mask();
    log(format_args!("Panique du noyau : {info}"));
    hal::cpu::halt()
}
