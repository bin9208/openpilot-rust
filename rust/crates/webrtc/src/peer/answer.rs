use super::{
    metadata::{configured, h264, projected},
    Peer,
};
use crate::{
    channel::Channel,
    network::{Gathered, Network},
    owner_graph::{mid, Graph},
    sender::Sender,
    video::{ipc::Camera, track::Track},
    Error,
};
use rtc::sansio::Protocol;
use rtc::{
    crypto::{self, SignatureScheme},
    media_stream::MediaStreamTrack,
    peer_connection::{
        certificate::{CertificateParams, RTCCertificate},
        sdp::RTCSessionDescription,
        state::RTCPeerConnectionState,
        transport::RTCIceCandidateInit,
    },
    rtp_transceiver::rtp_sender::{
        RTCRtpCodec, RTCRtpCodingParameters, RTCRtpEncodingParameters, RtpCodecKind,
    },
    sdp::description::{common::Attribute, session::SessionDescription},
};
use std::{collections::VecDeque, time::Instant};
use tokio::task::JoinSet;

fn prepared_owner(
    index: usize,
    parsed: &SessionDescription,
    graph: &Graph,
    tracks: &mut [(usize, Camera, Option<Track>)],
    sync: bool,
    certificate: RTCCertificate,
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
    rtc.prepare_remote_description(
        Instant::now(),
        RTCSessionDescription::offer(partition.marshal())?,
        graph.transport(parsed, index)?,
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

fn owner_answers(
    peers: &mut [Peer],
    mut gathered: Vec<(usize, Gathered)>,
) -> Result<Vec<SessionDescription>, Error> {
    let mut answers = Vec::new();
    for peer in peers {
        let gathered_index = gathered.iter().position(|(index, _)| *index == peer.index);
        let value = gathered_index.map(|index| &gathered[index].1);
        if let Some(value) = value {
            for candidate in &value.candidates {
                peer.rtc.add_local_candidate(candidate.clone())?;
            }
            peer.rtc
                .add_local_candidate(RTCIceCandidateInit::default())?;
        }
        let answer = peer.rtc.create_answer(None)?;
        peer.rtc
            .set_local_description(Instant::now(), answer.clone())?;
        let mut parsed_answer = answer.unmarshal()?;
        for sender in &mut peer.senders {
            let id = peer
                .rtc
                .rtp_sender(sender.id)
                .ok_or(Error::Contract("sender disappeared"))?
                .track()
                .track_id()
                .clone();
            sender.active = parsed_answer.media_descriptions.iter().any(|media| {
                media.attributes.iter().any(|attribute| {
                    attribute.key == "msid"
                        && attribute
                            .value
                            .as_deref()
                            .is_some_and(|value| value.ends_with(&id))
                }) && media.attribute("inactive").is_none()
                    && media.attribute("recvonly").is_none()
            });
        }
        super::metadata::source_cname(&mut parsed_answer, &peer.cname);
        projected(&mut parsed_answer, value)?;
        answers.push(parsed_answer);
        if let Some(index) = gathered_index {
            let (_, value) = gathered.swap_remove(index);
            peer.gathered = !value.candidates.is_empty();
            peer.sockets = value.sockets;
        } else {
            peer.rtc.close()?;
        }
    }
    Ok(answers)
}

pub(crate) async fn prepare(
    parsed: &SessionDescription,
    graph: &Graph,
    tracks: Vec<(Camera, Track)>,
    sync: bool,
    network: &Network,
) -> Result<(Vec<Peer>, RTCSessionDescription), Error> {
    let certificate = RTCCertificate::generate(
        crypto::default_provider()?.crypto(),
        SignatureScheme::EcdsaP256Sha256,
        CertificateParams::new(vec!["WebRTC".to_owned()])
            .map_err(|error| Error::Certificate(error.to_string()))?,
    )?;
    let video_indices: Vec<_> = parsed
        .media_descriptions
        .iter()
        .enumerate()
        .filter_map(|(index, media)| (media.media_name.media == "video").then_some(index))
        .collect();
    let mut tracks: Vec<_> = tracks
        .into_iter()
        .enumerate()
        .map(|(index, (camera, track))| {
            video_indices
                .get(index)
                .copied()
                .map(|media| (media, camera, Some(track)))
                .ok_or(Error::Contract("missing video transceiver"))
        })
        .collect::<Result<_, _>>()?;
    let mut peers = Vec::new();
    let mut gathering = JoinSet::new();
    for (index, selection) in graph.owners.iter().enumerate() {
        let Some(peer) =
            prepared_owner(index, parsed, graph, &mut tracks, sync, certificate.clone())?
        else {
            continue;
        };
        peers.push(peer);
        if !selection.stopped {
            let network = network.clone();
            gathering.spawn(async move { (index, network.gather().await) });
        }
    }
    let mut gathered = Vec::new();
    while let Some(result) = gathering.join_next().await {
        let (index, result) =
            result.map_err(|_| Error::Contract("candidate gathering task failed"))?;
        gathered.push((index, result?));
    }
    let cname = uuid::Uuid::new_v4().to_string();
    for peer in &mut peers {
        peer.cname.clone_from(&cname);
        for sender in &mut peer.senders {
            sender.set_cname(&cname);
        }
    }
    let answers = owner_answers(&mut peers, gathered)?;
    let mut merged = answers
        .first()
        .ok_or(Error::Contract("offer has no transport owners"))?
        .clone();
    merged.media_descriptions = parsed
        .media_descriptions
        .iter()
        .map(|media| {
            let target = mid(media)?;
            answers
                .iter()
                .flat_map(|answer| &answer.media_descriptions)
                .find(|answer| answer.attribute("mid").flatten() == Some(target))
                .cloned()
                .ok_or(Error::Contract("owner answer omitted MID"))
        })
        .collect::<Result<_, _>>()?;
    merged.attributes.retain(|value| value.key != "group");
    merged.attributes.push(Attribute::new(
        "group".to_owned(),
        Some(format!(
            "BUNDLE {}",
            parsed
                .media_descriptions
                .iter()
                .map(mid)
                .collect::<Result<Vec<_>, _>>()?
                .join(" ")
        )),
    ));
    Ok((peers, RTCSessionDescription::answer(merged.marshal())?))
}
