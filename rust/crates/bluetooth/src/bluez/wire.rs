use super::Error;
use dbus::Message;

fn string(fields: &mut Vec<u8>, code: u8, value: &str) -> Result<(), Error> {
    fields.resize(fields.len().next_multiple_of(8), 0);
    fields.extend([code, 1, b's', 0]);
    fields.extend(
        u32::try_from(value.len())
            .map_err(|_| Error::Property("error header length"))?
            .to_le_bytes(),
    );
    fields.extend(value.as_bytes());
    fields.push(0);
    Ok(())
}

pub(super) fn error(call: &Message, name: &str) -> Result<Message, Error> {
    let mut fields = Vec::new();
    string(&mut fields, 4, name)?;
    fields.resize(fields.len().next_multiple_of(8), 0);
    fields.extend([5, 1, b'u', 0]);
    fields.extend(
        call.get_serial()
            .ok_or(Error::Property("reply serial"))?
            .to_le_bytes(),
    );
    if let Some(sender) = call.sender() {
        string(&mut fields, 6, &sender)?;
    }
    let mut bytes = vec![b'l', 3, 0, 1];
    bytes.extend(0_u32.to_le_bytes());
    bytes.extend(1_u32.to_le_bytes());
    bytes.extend(
        u32::try_from(fields.len())
            .map_err(|_| Error::Property("error header length"))?
            .to_le_bytes(),
    );
    bytes.extend(fields);
    bytes.resize(bytes.len().next_multiple_of(8), 0);
    let mut message = Message::demarshal(&bytes)?;
    message.set_serial(0);
    Ok(message)
}
