fn main() {
    let result = openpilot_runtime_installer::Options::parse(std::env::args_os().skip(1)).and_then(
        |options| {
            openpilot_runtime_installer::run(&options, &openpilot_runtime_installer::CurlDownload)
        },
    );
    match result {
        Ok(()) => println!("Runtime package verified for checkout"),
        Err(error) => {
            eprintln!("runtime installer: {error}");
            std::process::exit(1);
        }
    }
}
