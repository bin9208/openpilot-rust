use super::{
    metadata::{configured, h264},
    Peer,
};
use crate::{
    channel::Channel,
    owner_graph::{mid, Graph},
    sender::Sender,
    video::{ipc::Camera, track::Track},
    Error,
};
use rtc::{
    media_stream::MediaStreamTrack,
    peer_connection::{
        certificate::RTCCertificate, sdp::RTCSessionDescription, state::RTCPeerConnectionState,
    },
    rtp_transceiver::rtp_sender::{
        RTCRtpCodec, RTCRtpCodingParameters, RTCRtpEncodingParameters, RtpCodecKind,
    },
    sdp::description::session::SessionDescription,
};
use std::{collections::VecDeque, time::Instant};

pub(super) fn prepared_owner(
    index: usize,
    parsed: &SessionDescription,
    graph: &Graph,
    tracks: &mut [(usize, Camera, Option<Track>)],
    sync: bool,
    certificate: RTCCertificate,
    mut parameters: rtc::peer_connection::RTCRemoteTransportParameters,
) -> Result<Option<Peer>, Error> {
    let members: Vec<_> = graph
        .bindings
        .iter()
        .enumerate()
        .filter_map(|(media, owner)| (*owner == index).then_some(media))
        .collect();
    if members.is_empty() {
        return Ok(None);
    }
    let mut partition = parsed.clone();
    partition.media_descriptions = members
        .iter()
        .map(|index| parsed.media_descriptions[*index].clone())
        .collect();
    partition.attributes.retain(|value| value.key != "group");
    let mut rtc = configured(certificate)?;
    parameters.candidates.clear();
    rtc.prepare_remote_description(
        Instant::now(),
        RTCSessionDescription::offer(partition.marshal())?,
        parameters,
    )?;
    let mut senders = Vec::new();
    for (media, camera, track) in tracks.iter_mut() {
        if !members.contains(media) {
            continue;
        }
        let random = uuid::Uuid::new_v4();
        let ssrc = u32::from_be_bytes(random.as_bytes()[0..4].try_into()?);
        let debug = track.as_ref().is_some_and(Track::is_debug);
        let id = rtc.add_track(MediaStreamTrack::new(
            uuid::Uuid::new_v4().to_string(),
            format!("{}:{}", camera.name(), uuid::Uuid::new_v4()),
            camera.name().to_owned(),
            RtpCodecKind::Video,
            vec![RTCRtpEncodingParameters {
                rtp_coding_parameters: RTCRtpCodingParameters {
                    ssrc: Some(ssrc),
                    ..Default::default()
                },
                codec: if debug {
                    RTCRtpCodec::default()
                } else {
                    h264("42001f")
                },
                ..Default::default()
            }],
        ))?;
        senders.push(Sender::new(
            id,
            track
                .take()
                .ok_or(Error::Contract("camera assigned twice"))?,
            mid(&parsed.media_descriptions[*media])?,
            ssrc,
            !debug && sync && *camera == Camera::Road,
        )?);
    }
    Ok(Some(Peer {
        cname: String::new(),
        mdns: None,
        index,
        rtc,
        sockets: Vec::new(),
        state: RTCPeerConnectionState::New,
        active: false,
        gathered: false,
        senders,
        channel: Channel::default(),
        queued: VecDeque::new(),
    }))
}
