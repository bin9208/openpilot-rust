use super::Reply;
use openpilot_can::Frame;
use openpilot_card::isotp::Error;

pub(super) fn frame(reply: &Reply, payload: &[u8]) -> Result<Frame, Error> {
    let tx = reply.target.0;
    let response = if tx > 0x10000000 {
        i64::from((tx & 0xffff0000) + ((tx << 8) & 0xff00) + ((tx >> 8) & 0xff))
    } else {
        i64::from(tx) + reply.offset
    };
    let address = u32::try_from(response).map_err(|_| Error::Address(tx))?;
    let mut data = Vec::new();
    if let Some(subaddress) = reply.target.1 {
        data.push(subaddress);
    }
    data.extend_from_slice(payload);
    data.resize(8, 0);
    Ok(Frame {
        address,
        data,
        bus: reply.bus,
    })
}
pub(super) fn segmented(reply: &Reply) -> Result<Vec<Frame>, Error> {
    let size = if reply.target.1.is_some() { 7 } else { 8 };
    if reply.response.len() < size {
        let mut data = vec![u8::try_from(reply.response.len()).map_err(|_| Error::TxLength)?];
        data.extend_from_slice(&reply.response);
        return Ok(vec![frame(reply, &data)?]);
    }
    let mut data = (0x1000 | u16::try_from(reply.response.len()).map_err(|_| Error::TxLength)?)
        .to_be_bytes()
        .to_vec();
    data.extend_from_slice(&reply.response[..size - 2]);
    let mut result = vec![frame(reply, &data)?];
    for (index, data) in reply.response[size - 2..].chunks(size - 1).enumerate() {
        let mut payload = vec![0x20 | u8::try_from((index + 1) & 15).map_err(|_| Error::TxLength)?];
        payload.extend_from_slice(data);
        result.push(frame(reply, &payload)?);
    }
    Ok(result)
}
