use crate::{at::Apdu, codec::*, http::Es9, protocol, protocol::command, Result};
use serde_json::{json, Value};
pub fn list(client: &mut impl Apdu) -> Result<Vec<Value>> {
    let response = command(client, &encode(0xbf28, &[]))?;
    let root = require(&response, 0xbf28, "ListNotificationResponse")?;
    let Some(metadata) = find(root, 0xa0) else {
        return Ok(Vec::new());
    };
    tlvs(metadata).into_iter().filter(|t|t.tag==0xbf2f).map(|t|{
        let mut item=json!({"seqNumber":null,"profileManagementOperation":null,"notificationAddress":null,"iccid":null});
        for field in tlvs(t.value){match field.tag{
            0x80=>item["seqNumber"]=json!(integer(field.value)?),
            0x81=>item["profileManagementOperation"]=json!([(0x80,"install"),(0x40,"enable"),(0x20,"disable"),(0x10,"delete")].into_iter().find(|(mask,_)|field.value.get(1).is_some_and(|v|v&mask!=0)).map(|(_,name)|name).unwrap_or("unknown")),
            0x0c=>item["notificationAddress"]=json!(utf8_ignore(field.value)),
            0x5a=>item["iccid"]=json!(tbcd(field.value)),
            _=>{},
        }}Ok(item)
    }).collect()
}
pub fn process(client: &mut impl Apdu, http: &mut impl Es9) -> Result<()> {
    for item in list(client)? {
        let result = (|| {
            let seq = item["seqNumber"]
                .as_u64()
                .ok_or_else(|| protocol("Missing seqNumber"))?;
            let address = item["notificationAddress"]
                .as_str()
                .ok_or_else(|| protocol("Missing notificationAddress"))?;
            let response = command(
                client,
                &encode(0xbf2b, &encode(0xa0, &encode(0x80, &int_bytes(seq)))),
            )?;
            let inner = require(
                require(&response, 0xbf2b, "RetrieveNotificationsListResponse")?,
                0xa0,
                "RetrieveNotificationsListResponse",
            )?;
            let pending = tlvs(inner)
                .into_iter()
                .find(|t| matches!(t.tag, 0xbf37 | 0x30))
                .ok_or_else(|| protocol("Missing PendingNotification"))?;
            http.request(
                address,
                "handleNotification",
                json!({"pendingNotification":b64(pending.value)}),
                "HandleNotification",
            )?;
            let response = command(client, &encode(0xbf30, &encode(0x80, &int_bytes(seq))))?;
            let root = require(&response, 0xbf30, "NotificationSentResponse")?;
            if integer(require(root, 0x80, "RemoveNotificationFromList status")?)? != 0 {
                return Err(protocol("RemoveNotificationFromList failed"));
            }
            Ok(())
        })();
        if let Err(error) = result {
            eprintln!("notification {} failed: {error}", item["seqNumber"]);
        }
    }
    Ok(())
}
