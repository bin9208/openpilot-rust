use crate::{
    network::Network,
    owner_graph::Graph,
    peer::{self, Peer},
    request::{ClientKey, StreamRequest},
    runtime::Profile,
    video::{ipc::Camera, track::Track},
    Error,
};
use openpilot_messaging::{runtime::SubMaster, state::Options};
use rtc::{
    peer_connection::sdp::RTCSessionDescription, sdp::description::session::SessionDescription,
};
use std::time::Instant;

mod bridge;
mod compact;
mod lifecycle;

pub(crate) use bridge::Publishers;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Standard,
    Carrot,
    StandardDebug,
    CarrotDebug,
}

struct Lifecycle {
    ready: bool,
    connected_once: bool,
    closed: bool,
}

pub(crate) struct Session {
    pub identifier: String,
    pub client_key: ClientKey,
    pub road: bool,
    pub created: Instant,
    pub peers: Vec<Peer>,
    graph: Option<Graph>,
    parsed: SessionDescription,
    tracks: Vec<(Camera, Track)>,
    incoming: Vec<String>,
    outgoing: Option<SubMaster>,
    compact: Option<compact::Compact>,
    outgoing_alive: bool,
    mode: Mode,
    sync: bool,
    lifecycle: Lifecycle,
    expected_incoming: usize,
    cursor: usize,
    activation: Vec<usize>,
    disconnected: Option<Instant>,
    last_outgoing: Instant,
}

fn direction(media: &rtc::sdp::description::media::MediaDescription, name: &str) -> bool {
    media.attribute(name).is_some()
}

impl Session {
    const fn is_carrot(&self) -> bool {
        matches!(self.mode, Mode::Carrot | Mode::CarrotDebug)
    }

    const fn is_debug(&self) -> bool {
        matches!(self.mode, Mode::StandardDebug | Mode::CarrotDebug)
    }

    pub fn new(request: &StreamRequest, remote: &str, profile: Profile) -> Result<Self, Error> {
        let carrot = profile.carrot;
        let parsed = RTCSessionDescription::offer(request.sdp.clone())?.unmarshal()?;
        let expected = parsed
            .media_descriptions
            .iter()
            .filter(|media| {
                media.media_name.media == "video"
                    && (direction(media, "recvonly") || direction(media, "sendrecv"))
            })
            .count();
        if request.cameras.len() != expected {
            return Err(Error::Contract(
                "Incoming stream has misconfigured number of video tracks",
            ));
        }
        let mut tracks = Vec::new();
        for camera in &request.cameras {
            if tracks.iter().any(|(existing, _)| existing == camera) {
                return Err(Error::Contract("duplicate camera"));
            }
            tracks.push((*camera, Track::new(*camera, carrot, profile.debug)?));
        }
        let outgoing = if request.outgoing.is_empty() {
            None
        } else {
            Some(SubMaster::for_runtime(
                &request
                    .outgoing
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                Options::default(),
            )?)
        };
        let expected_incoming = parsed
            .media_descriptions
            .iter()
            .filter(|media| {
                media.media_name.media == "application"
                    || direction(media, "sendonly")
                    || direction(media, "sendrecv")
            })
            .count();
        Ok(Self {
            identifier: uuid::Uuid::new_v4().to_string(),
            client_key: request.key(remote),
            road: request.cameras.contains(&Camera::Road),
            created: Instant::now(),
            peers: Vec::new(),
            graph: None,
            parsed,
            tracks,
            incoming: request.incoming.clone(),
            outgoing,
            compact: if request.carrot_state && !carrot {
                Some(compact::Compact::new()?)
            } else {
                None
            },
            outgoing_alive: true,
            mode: match (carrot, profile.debug) {
                (false, false) => Mode::Standard,
                (true, false) => Mode::Carrot,
                (false, true) => Mode::StandardDebug,
                (true, true) => Mode::CarrotDebug,
            },
            sync: carrot && request.carrot_state,
            lifecycle: Lifecycle {
                ready: false,
                connected_once: false,
                closed: false,
            },
            expected_incoming,
            cursor: 0,
            activation: Vec::new(),
            disconnected: None,
            last_outgoing: Instant::now(),
        })
    }

    pub async fn answer(&mut self, network: &Network) -> Result<RTCSessionDescription, Error> {
        if !self.is_debug() && !self.tracks.is_empty() {
            for media in &mut self.parsed.media_descriptions {
                if media.media_name.media != "video" {
                    continue;
                }
                let payloads: Vec<_> = media
                    .attributes
                    .iter()
                    .filter(|attribute| attribute.key == "rtpmap")
                    .filter_map(|attribute| attribute.value.as_deref()?.split_once(' '))
                    .filter_map(|(payload, codec)| {
                        codec.starts_with("H264/").then_some(payload.to_owned())
                    })
                    .collect();
                if payloads.is_empty() {
                    return Err(Error::Contract(
                        "None of the preferred codecs is supported in remote SDP",
                    ));
                }
                media
                    .media_name
                    .formats
                    .retain(|payload| payloads.contains(payload));
                media.attributes.retain(|attribute| {
                    !matches!(attribute.key.as_str(), "rtpmap" | "fmtp" | "rtcp-fb")
                        || attribute.value.as_deref().is_some_and(|value| {
                            payloads
                                .iter()
                                .any(|payload| value.split_whitespace().next() == Some(payload))
                        })
                });
            }
        }
        let graph = Graph::new(&self.parsed)?;
        self.activation = Graph::consumers(&self.parsed)
            .into_iter()
            .map(|index| graph.bindings[index])
            .collect();
        let (peers, answer) = peer::prepare(
            &self.parsed,
            &graph,
            std::mem::take(&mut self.tracks),
            self.sync,
            network,
        )
        .await?;
        self.peers = peers;
        self.graph = Some(graph);
        Ok(answer)
    }
}
