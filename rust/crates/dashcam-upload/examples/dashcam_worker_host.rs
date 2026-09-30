#[path = "../src/driver.rs"]
mod driver;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let executable = std::env::args_os()
        .nth(1)
        .ok_or("worker executable required")?;
    driver::run(executable.into())
}
