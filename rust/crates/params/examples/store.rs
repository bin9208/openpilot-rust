use openpilot_params::{Params, KEYS};
use std::{
    env,
    error::Error,
    io::{self, Read, Write},
    path::Path,
};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args().collect();
    if args.len() != 5 {
        return Err("usage: store ROOT PREFIX OP KEY_OR_MASK".into());
    }
    let params = Params::open(Path::new(&args[1]), &args[2])?;
    match args[3].as_str() {
        "put" => {
            let mut bytes = Vec::new();
            io::stdin().read_to_end(&mut bytes)?;
            params.put(&args[4], &bytes)?;
        }
        "get" => {
            if let Some(bytes) = params.get(&args[4])? {
                io::stdout().write_all(&bytes)?;
            }
        }
        "remove" => params.remove(&args[4])?,
        "clear" => params.clear(args[4].parse()?)?,
        "catalog" => {
            for key in KEYS {
                println!(
                    "{}\t{}\t{}\t{}\t{}",
                    key.name,
                    key.flags,
                    key.kind,
                    u8::from(key.default.is_some()),
                    key.default.unwrap_or("")
                );
            }
        }
        _ => return Err("unknown operation".into()),
    }
    Ok(())
}
