use crate::{
    args::{Action, parse},
    docs,
    engine::Stream,
};
use alloc::{format, vec::Vec};
use nova_cli::runtime as rt;
use nova_exec_client::Launch;

fn execute(launch: &Launch) -> rt::Result<()> {
    let args: Vec<_> = launch.argv.iter().map(|s| s.as_bytes()).collect();
    let action = parse(&args, launch.utf8, launch.posix_options).map_err(|error| {
        let _ = launch.write_stderr(format!("cut: {error}\n").as_bytes());
        rt::BAD_ARGUMENT
    })?;
    let (config, files) = match action {
        Action::Run(config, files) => (config, files),
        doc => {
            return launch
                .write_stdout(docs::text(matches!(doc, Action::Help), launch.french).as_bytes());
        }
    };
    let mut buf = [0; 1024];
    let mut output = Vec::new();
    for path in files {
        let mut stream = Stream::new(&config);
        launch.with_input(path, |read| {
            loop {
                let n = read(&mut buf)?;
                output.clear();
                if n == 0 {
                    stream.finish(&mut output);
                } else {
                    stream.feed(&buf[..n], &mut output);
                }
                launch.write_stdout(&output)?;
                if n == 0 {
                    break Ok(());
                }
            }
        })?;
    }
    Ok(())
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    let result = Launch::current().and_then(|launch| {
        let result = execute(&launch);
        if let Err(reason) = result
            && reason != rt::BAD_ARGUMENT
        {
            let _ = launch
                .write_stderr(format!("cut: {}\n", nova_cli::console::reason(reason)).as_bytes());
        }
        result
    });
    rt::exit(result.err().unwrap_or(0))
}
