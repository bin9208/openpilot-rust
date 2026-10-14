fn main() {
    let mut args = std::env::args_os().skip(1).peekable();
    let mode = args
        .peek()
        .and_then(|arg| arg.to_str())
        .filter(|arg| *arg == "--stage-update" || *arg == "--adopt-update")
        .map(str::to_owned);
    let result = if let Some(mode) = mode {
        args.next();
        let mut arguments: Vec<_> = args.collect();
        if mode == "--adopt-update" {
            arguments.extend(["--target".into(), "HEAD".into()]);
        }
        openpilot_runtime_installer::transaction::StageOptions::parse(arguments.into_iter())
            .and_then(|options| {
                let sha = if mode == "--adopt-update" {
                    openpilot_runtime_installer::transaction::adopt(
                        options,
                        &openpilot_runtime_installer::CurlDownload,
                    )?
                } else {
                    openpilot_runtime_installer::transaction::prepare(
                        options,
                        &openpilot_runtime_installer::CurlDownload,
                    )?
                };
                println!("Complete native source/package update staged for {sha}; reboot required");
                Ok(())
            })
    } else {
        openpilot_runtime_installer::Options::parse(args).and_then(|options| {
            openpilot_runtime_installer::run(&options, &openpilot_runtime_installer::CurlDownload)
        })
    };
    match result {
        Ok(()) => println!("Runtime package verified for checkout"),
        Err(error) => {
            eprintln!("runtime installer: {error}");
            std::process::exit(1);
        }
    }
}
