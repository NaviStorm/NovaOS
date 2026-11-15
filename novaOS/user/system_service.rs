//! Outils système embarqués. Une incarnation ne possède que sa paire de canaux
//! et sa vue temporaire dédiée.
#![no_std]
#![no_main]
#[path = "services/node.rs"]
mod node;
#[path = "services/protocol.rs"]
mod protocol;
#[path = "storage/runtime.rs"]
mod runtime;
#[allow(dead_code)]
#[path = "../abi/service_process.rs"]
mod service_process;

#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    if runtime::config(2) == 0 && matches!(runtime::config(0), 100 | 101) {
        runtime::exit(node::worker(runtime::config(0)).err().unwrap_or(0));
    }
    runtime::exit(node::serve(false, |_, _| unreachable!()).err().unwrap_or(0))
}
