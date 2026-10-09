#[path = "replies.rs"]
mod replies;
use bytes::BytesMut;
use openpilot_webrtc::Error;
use rtc::{
    crypto::{default_provider, RTCCryptoProvider},
    ice::{
        agent::{agent_config::AgentConfig, Agent},
        candidate::{candidate_host::CandidateHostConfig, Candidate, CandidateConfig},
    },
    sansio::Protocol,
    shared::{TaggedBytesMut, TransportContext, TransportProtocol},
    stun::{
        attributes::ATTR_USERNAME,
        error_code::ErrorCode,
        fingerprint::FINGERPRINT,
        integrity::MessageIntegrity,
        message::{
            Getter, Message, TransactionId, BINDING_ERROR, BINDING_REQUEST, BINDING_SUCCESS,
        },
        textattrs::Username,
        xoraddr::XorMappedAddress,
    },
};
use serde_json::{json, Value};
use std::{
    fmt::Write,
    net::{SocketAddr, UdpSocket},
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
pub struct Config {
    pub locals: usize,
    pub remotes: usize,
    pub source: bool,
    pub controlling: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            locals: 1,
            remotes: 1,
            source: true,
            controlling: true,
        }
    }
}

pub struct Request {
    pub message: Message,
    pub from: SocketAddr,
    pub remote: usize,
}

#[derive(Clone, Copy)]
pub enum Reply {
    Success,
    Error(ErrorCode),
}

pub struct Rig {
    pub agent: Agent,
    pub locals: Vec<UdpSocket>,
    remotes: Vec<UdpSocket>,
    crypto: Arc<dyn RTCCryptoProvider>,
    started: Instant,
    pub wire: Vec<Value>,
}

pub const LOCAL_PASSWORD: &str = "local-source-240-password-owned";
const REMOTE_PASSWORD: &str = "remote-source-240-password-owned";

fn candidate(address: SocketAddr) -> Result<Candidate, Error> {
    Ok(CandidateHostConfig {
        base_config: CandidateConfig {
            network: "udp".to_owned(),
            address: address.ip().to_string(),
            port: address.port(),
            component: 1,
            priority: 2_130_706_431,
            ..Default::default()
        },
        ..Default::default()
    }
    .new_candidate_host()?)
}

fn socket() -> Result<UdpSocket, Error> {
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    socket.set_read_timeout(Some(Duration::from_secs(1)))?;
    Ok(socket)
}

impl Rig {
    pub fn new(config: Config) -> Result<Self, Error> {
        let crypto = default_provider()?;
        let started = Instant::now();
        let mut agent = Agent::new(
            started,
            Arc::new(AgentConfig {
                local_ufrag: "local240".to_owned(),
                local_pwd: LOCAL_PASSWORD.to_owned(),
                ..Default::default()
            }),
            Arc::clone(&crypto),
        )?;
        if config.source {
            agent.enable_source_checks();
        }
        let locals = (0..config.locals)
            .map(|_| socket())
            .collect::<Result<Vec<_>, _>>()?;
        let remotes = (0..config.remotes)
            .map(|_| socket())
            .collect::<Result<Vec<_>, _>>()?;
        for socket in &locals {
            agent.add_local_candidate(candidate(socket.local_addr()?)?)?;
        }
        for socket in &remotes {
            agent.add_remote_candidate(candidate(socket.local_addr()?)?)?;
        }
        agent.start_connectivity_checks(
            Instant::now(),
            config.controlling,
            "remote240".to_owned(),
            REMOTE_PASSWORD.to_owned(),
        )?;
        Ok(Self {
            agent,
            locals,
            remotes,
            crypto,
            started,
            wire: Vec::new(),
        })
    }

    fn record(&mut self, phase: &str, packet: &TaggedBytesMut) -> Result<(), Error> {
        let mut bytes = String::new();
        for byte in &packet.message {
            write!(bytes, "{byte:02x}")?;
        }
        self.wire.push(json!({"phase":phase,"seconds":self.started.elapsed().as_secs_f64(),
            "local":packet.transport.local_addr.to_string(),"peer":packet.transport.peer_addr.to_string(),"hex":bytes}));
        Ok(())
    }

    pub fn writes(&mut self) -> Result<Vec<Request>, Error> {
        self.agent.handle_timeout(Instant::now())?;
        let mut requests = Vec::new();
        while let Some(packet) = self.agent.poll_write() {
            self.record("provider-send", &packet)?;
            let socket = self
                .locals
                .iter()
                .find(|socket| {
                    socket
                        .local_addr()
                        .is_ok_and(|address| address == packet.transport.local_addr)
                })
                .ok_or(Error::Contract("owned sender socket absent"))?;
            socket.send_to(&packet.message, packet.transport.peer_addr)?;
            let index = self
                .remotes
                .iter()
                .position(|socket| {
                    socket
                        .local_addr()
                        .is_ok_and(|address| address == packet.transport.peer_addr)
                })
                .ok_or(Error::Contract("owned recipient absent"))?;
            let mut buffer = [0_u8; 2048];
            let (length, from) = self.remotes[index].recv_from(&mut buffer)?;
            let mut message = Message::new();
            message.unmarshal_binary(&buffer[..length])?;
            requests.push(Request {
                message,
                from,
                remote: index,
            });
        }
        Ok(requests)
    }

    pub fn read(&mut self, local: usize) -> Result<Option<String>, Error> {
        let mut buffer = [0_u8; 2048];
        let (length, peer_addr) = self.locals[local].recv_from(&mut buffer)?;
        let packet = TaggedBytesMut {
            now: Instant::now(),
            transport: TransportContext {
                local_addr: self.locals[local].local_addr()?,
                peer_addr,
                ecn: None,
                transport_protocol: TransportProtocol::UDP,
            },
            message: BytesMut::from(&buffer[..length]),
        };
        self.record("provider-receive", &packet)?;
        Ok(self
            .agent
            .handle_read(packet)
            .err()
            .map(|error| error.to_string()))
    }

    pub fn wrong_local(&self, request: &mut Request, local: usize) -> Result<(), Error> {
        request.from = self.locals[local].local_addr()?;
        Ok(())
    }

    pub fn finish(mut self, scenario: &str) -> Result<Value, Error> {
        let state = self.agent.state().to_string();
        let pairs: Vec<_> = self
            .agent
            .get_candidate_pairs_stats(Instant::now())
            .iter()
            .map(|pair| format!("{:?}", pair.state))
            .collect();
        self.agent.close()?;
        Ok(json!({"scenario":scenario,"state":state,"pairs":pairs,"wire":self.wire}))
    }
}
