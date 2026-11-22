#![no_std]
#![no_main]
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    nova_exec_client::admin_cli::main(nova_exec_client::admin_cli::Program::Usermod)
}
