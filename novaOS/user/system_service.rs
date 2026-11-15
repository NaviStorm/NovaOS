//! Outils système embarqués. Une incarnation ne possède que sa paire de canaux
//! et sa vue temporaire dédiée.
#![no_std]
#![no_main]
#[path = "services/cache_actor.rs"]
mod cache_actor;
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
    let mut cache = cache_actor::Client::new();
    runtime::exit(
        node::serve_extension(
            false,
            |_, _| unreachable!(),
            |id, op, step, input| cache.handle(id, op, step, input),
        )
        .err()
        .unwrap_or(0),
    )
}
