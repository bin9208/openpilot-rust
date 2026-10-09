use super::{
    json, BytesMut, Error, Getter, Instant, Message, MessageIntegrity, Reply, Request, Rig,
    TaggedBytesMut, TransactionId, TransportContext, TransportProtocol, Username, Value,
    XorMappedAddress, ATTR_USERNAME, BINDING_ERROR, BINDING_REQUEST, BINDING_SUCCESS, FINGERPRINT,
    LOCAL_PASSWORD, REMOTE_PASSWORD,
};

impl Rig {
    pub fn response(
        &mut self,
        request: &Request,
        id: TransactionId,
        reply: Reply,
    ) -> Result<(), Error> {
        let mut message = Message::new();
        match reply {
            Reply::Success => message.build(&[
                Box::new(BINDING_SUCCESS),
                Box::new(id),
                Box::new(XorMappedAddress {
                    ip: request.from.ip(),
                    port: request.from.port(),
                }),
                Box::new(MessageIntegrity::new_short_term_integrity_with_provider(
                    REMOTE_PASSWORD.to_owned(),
                    self.crypto.crypto(),
                )),
                Box::new(FINGERPRINT),
            ])?,
            Reply::Error(code) => message.build(&[
                Box::new(BINDING_ERROR),
                Box::new(id),
                Box::new(code),
                Box::new(MessageIntegrity::new_short_term_integrity_with_provider(
                    REMOTE_PASSWORD.to_owned(),
                    self.crypto.crypto(),
                )),
                Box::new(FINGERPRINT),
            ])?,
        }
        self.remotes[request.remote].send_to(&message.raw, request.from)?;
        self.record(
            "owned-reply",
            &TaggedBytesMut {
                now: Instant::now(),
                transport: TransportContext {
                    local_addr: self.remotes[request.remote].local_addr()?,
                    peer_addr: request.from,
                    ecn: None,
                    transport_protocol: TransportProtocol::UDP,
                },
                message: BytesMut::from(message.raw.as_slice()),
            },
        )
    }

    pub fn signed_bad_request(&mut self, bad_username: bool) -> Result<Value, Error> {
        let mut request = Message::new();
        request.build(&[
            Box::new(BINDING_REQUEST),
            Box::new(TransactionId::new()),
            Box::new(Username::new(
                ATTR_USERNAME,
                if bad_username {
                    "wrong"
                } else {
                    "local240:remote240"
                }
                .to_owned(),
            )),
            Box::new(MessageIntegrity::new_short_term_integrity_with_provider(
                if bad_username {
                    LOCAL_PASSWORD
                } else {
                    "wrong-password"
                }
                .to_owned(),
                self.crypto.crypto(),
            )),
            Box::new(FINGERPRINT),
        ])?;
        self.remotes[0].send_to(&request.raw, self.locals[0].local_addr()?)?;
        assert!(self.read(0)?.is_none());
        let outputs = self.writes()?;
        let response = outputs
            .iter()
            .find(|output| output.message.typ == BINDING_ERROR)
            .ok_or(Error::Contract("signed400 reply absent"))?;
        FINGERPRINT.check(&response.message)?;
        MessageIntegrity::check(
            &mut response.message.clone(),
            LOCAL_PASSWORD.as_bytes(),
            self.crypto.crypto(),
        )?;
        let mut code = rtc::stun::error_code::ErrorCodeAttribute::default();
        code.get_from(&response.message)?;
        Ok(
            json!({"bad_username":bad_username,"transaction_matches":response.message.transaction_id==request.transaction_id,"error":code.code.0,"integrity_verified":true,"fingerprint_verified":true}),
        )
    }
}
