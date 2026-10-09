use super::{metadata::projected, owner::prepared_owner, Peer};
use crate::{
    network::{Gathered, Network},
    owner_graph::{mid, Graph},
    video::{ipc::Camera, track::Track},
    Error,
};
use rtc::sansio::Protocol;
use rtc::{
    crypto::{self, SignatureScheme},
    peer_connection::{
        certificate::{CertificateParams, RTCCertificate},
        sdp::RTCSessionDescription,
        transport::RTCIceCandidateInit,
    },
    sdp::description::{common::Attribute, session::SessionDescription},
};
use std::time::Instant;
use tokio::task::JoinSet;

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

async fn remote_owners(
    parsed: &SessionDescription,
    graph: &Graph,
    network: &Network,
) -> Result<std::collections::HashMap<usize, crate::network::Remote>, Error> {
    let mut remote_tasks = JoinSet::new();
    for index in 0..graph.owners.len() {
        if graph.bindings.contains(&index) {
            let parameters = graph.transport(parsed, index)?;
            let network = network.clone();
            remote_tasks.spawn(async move { (index, network.resolve(parameters).await) });
        }
    }
    let mut remotes = std::collections::HashMap::new();
    while let Some(result) = remote_tasks.join_next().await {
        let (index, remote) =
            result.map_err(|_| Error::Contract("remote candidate task failed"))?;
        remotes.insert(index, remote?);
    }
    Ok(remotes)
}

async fn gather_owners(
    parsed: &SessionDescription,
    graph: &Graph,
    network: &Network,
    peers: &mut [Peer],
) -> Result<Vec<(usize, Gathered)>, Error> {
    let mut gathering = JoinSet::new();
    let mut remotes = remote_owners(parsed, graph, network).await?;
    for peer in peers {
        let remote = remotes
            .remove(&peer.index)
            .ok_or(Error::Contract("remote owner disappeared"))?;
        for candidate in remote.parameters.candidates {
            peer.rtc.add_remote_candidate(candidate)?;
        }
        peer.mdns = remote.lease;
        if !graph.owners[peer.index].stopped {
            let index = peer.index;
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
    Ok(gathered)
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
    for index in 0..graph.owners.len() {
        if !graph.bindings.contains(&index) {
            continue;
        }
        let parameters = graph.transport(parsed, index)?;
        let Some(peer) = prepared_owner(
            index,
            parsed,
            graph,
            &mut tracks,
            sync,
            certificate.clone(),
            parameters,
        )?
        else {
            continue;
        };
        peers.push(peer);
    }
    let gathered = gather_owners(parsed, graph, network, &mut peers).await?;
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
