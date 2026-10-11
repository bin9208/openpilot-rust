use openpilot_usbgpu::{
    native_usb::{Configuration, Scanner, Usb},
    transport::{Setup, Transport},
    Error,
};

#[derive(serde::Serialize)]
struct Observation {
    found: Option<u16>,
    scan_again: Option<u16>,
    configuration: Option<Configuration>,
    error_code: Option<i32>,
    error: Option<String>,
}

fn configuration(product: u16) -> Result<Configuration, Error> {
    let mut usb = Usb::open(0x1cbe, product, 0)?.ok_or(Error::Contract("USB device not found"))?;
    let active = usb.setup(Setup::KernelActive, 0, 0)?;
    if active == 1 {
        let code = usb.setup(Setup::Detach, 0, 0)?;
        assert_eq!(code, 0);
    }
    let configuration = usb.configuration(0)?;
    assert_eq!(
        usb.setup(Setup::Configuration, i32::from(configuration.value), 0)?,
        0
    );
    assert_eq!(usb.setup(Setup::Claim, 0, 0)?, 0);
    Ok(configuration)
}

fn find(scanner: &Scanner) -> Result<Option<u16>, Error> {
    for product in [0x92, 0x123] {
        if scanner.find(0x1cbe, product)?.is_some() {
            return Ok(Some(product));
        }
    }
    Ok(None)
}

fn main() -> Result<(), Error> {
    if std::env::args().nth(1).as_deref() == Some("--missing-symbols") {
        let scanner = Scanner::new()?;
        assert!(scanner.find(0xadd1, 1)?.is_some());
        let usb = Usb::open(0xadd1, 1, 0)?.unwrap();
        assert!(matches!(usb.configuration(0), Err(Error::Library(_))));
        drop(usb);
        drop(scanner);
        println!(
            "PASS: discovery/open work without optional config symbols; only metadata query fails."
        );
        return Ok(());
    }
    let scanner = Scanner::new()?;
    let found = find(&scanner)?;
    let scan_again = find(&scanner)?;
    let mut observation = Observation {
        found,
        scan_again,
        configuration: None,
        error_code: None,
        error: None,
    };
    if let Some(product) = found {
        match configuration(product) {
            Ok(value) => observation.configuration = Some(value),
            Err(error) => {
                if let Error::UsbApi { code, .. } = error {
                    observation.error_code = Some(code);
                }
                observation.error = Some(error.to_string());
            }
        }
    }
    drop(scanner);
    println!("{}", serde_json::to_string(&observation)?);
    Ok(())
}
