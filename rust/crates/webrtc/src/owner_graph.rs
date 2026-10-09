use crate::Error;
use rtc::peer_connection::transport::{
    RTCDtlsFingerprint, RTCDtlsParameters, RTCDtlsRole, RTCIceCandidateInit, RTCIceParameters,
    RTCIceRole,
};
use rtc::peer_connection::RTCRemoteTransportParameters;
use rtc::sdp::description::{media::MediaDescription, session::SessionDescription};

pub(crate) struct Selection {
    pub first_declared: usize,
    pub candidate_media: Option<usize>,
    pub local_role: RTCDtlsRole,
    pub stopped: bool,
}

pub(crate) struct Graph {
    pub bindings: Vec<usize>,
    pub owners: Vec<Selection>,
}

pub(crate) fn mid(media: &MediaDescription) -> Result<&str, Error> {
    media
        .attribute("mid")
        .flatten()
        .ok_or(Error::Contract("missing MID"))
}

fn attribute<'a>(parsed: &'a SessionDescription, index: usize, name: &str) -> Option<&'a str> {
    parsed.media_descriptions[index]
        .attribute(name)
        .flatten()
        .or_else(|| parsed.attribute(name).map(String::as_str))
}

fn ice(parsed: &SessionDescription, index: usize) -> Result<RTCIceParameters, Error> {
    let username = attribute(parsed, index, "ice-ufrag")
        .filter(|value| !value.is_empty())
        .ok_or(Error::Contract(
            "ICE username fragment or password is missing",
        ))?;
    let password = attribute(parsed, index, "ice-pwd")
        .filter(|value| !value.is_empty())
        .ok_or(Error::Contract(
            "ICE username fragment or password is missing",
        ))?;
    Ok(RTCIceParameters {
        username_fragment: username.to_owned(),
        password: password.to_owned(),
        ice_lite: parsed.attribute("ice-lite").is_some(),
    })
}

impl Graph {
    pub fn consumers(parsed: &SessionDescription) -> Vec<usize> {
        let media = &parsed.media_descriptions;
        media
            .iter()
            .enumerate()
            .filter_map(|(index, media)| {
                matches!(media.media_name.media.as_str(), "audio" | "video").then_some(index)
            })
            .chain(media.iter().enumerate().filter_map(|(index, media)| {
                (media.media_name.media == "application").then_some(index)
            }))
            .collect()
    }
    pub fn new(parsed: &SessionDescription) -> Result<Self, Error> {
        let mut result = Self {
            bindings: Vec::new(),
            owners: Vec::new(),
        };
        let mut bundled = Vec::new();
        for (index, media) in parsed.media_descriptions.iter().enumerate() {
            ice(parsed, index)?;
            if matches!(media.media_name.media.as_str(), "audio" | "video")
                && media.attribute("rtcp-mux").is_none()
            {
                return Err(Error::Contract("RTCP mux is not enabled"));
            }
            if !matches!(
                media.media_name.media.as_str(),
                "audio" | "video" | "application"
            ) {
                return Err(Error::Contract("unsupported media kind"));
            }
            let existing = result.owners.iter().position(|owner: &Selection| {
                parsed.media_descriptions[owner.first_declared]
                    .media_name
                    .media
                    == media.media_name.media
            });
            let owner = existing.unwrap_or(result.owners.len());
            if existing.is_none() {
                result.owners.push(Selection {
                    first_declared: index,
                    candidate_media: None,
                    local_role: RTCDtlsRole::Client,
                    stopped: false,
                });
            }
            result.bindings.push(owner);
            bundled.push(existing.is_some());
            result.owners[owner].candidate_media = Some(index);
            if attribute(parsed, index, "setup") == Some("active") {
                result.owners[owner].local_role = RTCDtlsRole::Server;
            }
        }
        if let Some(bundle) = parsed
            .attributes
            .iter()
            .find(|value| {
                value.key == "group"
                    && value
                        .value
                        .as_deref()
                        .is_some_and(|group| group.starts_with("BUNDLE "))
            })
            .and_then(|value| value.value.as_deref())
        {
            let mids: Vec<_> = bundle.split_whitespace().skip(1).collect();
            let mut master = None;
            for (index, media) in parsed.media_descriptions.iter().enumerate() {
                if Some(mid(media)?) == mids.first().copied() {
                    master = Some(index);
                    break;
                }
            }
            let master = master.ok_or(Error::Contract("BUNDLE master not found"))?;
            let master_owner = result.bindings[master];
            for (index, media) in parsed.media_descriptions.iter().enumerate() {
                if mids
                    .iter()
                    .skip(1)
                    .any(|value| media.attribute("mid").flatten() == Some(*value))
                    && !bundled[index]
                {
                    let old = result.bindings[index];
                    result.owners[old].stopped = true;
                    result.owners[old].candidate_media = None;
                    result.bindings[index] = master_owner;
                    bundled[index] = true;
                }
            }
        }
        Ok(result)
    }

    pub fn transport(
        &self,
        parsed: &SessionDescription,
        owner: usize,
    ) -> Result<RTCRemoteTransportParameters, Error> {
        let selection = &self.owners[owner];
        let consumer = Self::consumers(parsed)
            .into_iter()
            .find(|index| self.bindings[*index] == owner)
            .ok_or(Error::Contract("owner has no remaining consumer"))?;
        let remote_role = match attribute(parsed, consumer, "setup").unwrap_or("actpass") {
            "actpass" => RTCDtlsRole::Auto,
            "active" => RTCDtlsRole::Client,
            "passive" => RTCDtlsRole::Server,
            _ => return Err(Error::Contract("invalid setup")),
        };
        let mut fingerprints = Vec::new();
        for media in &parsed.media_descriptions[consumer].attributes {
            if media.key == "fingerprint" {
                if let Some((algorithm, value)) = media
                    .value
                    .as_deref()
                    .and_then(|value| value.split_once(' '))
                {
                    fingerprints.push(RTCDtlsFingerprint {
                        algorithm: algorithm.to_owned(),
                        value: value.to_owned(),
                    });
                }
            }
        }
        if fingerprints.is_empty() {
            if let Some((algorithm, value)) = parsed
                .attribute("fingerprint")
                .and_then(|value| value.split_once(' '))
            {
                fingerprints.push(RTCDtlsFingerprint {
                    algorithm: algorithm.to_owned(),
                    value: value.to_owned(),
                });
            }
        }
        let candidates = selection
            .candidate_media
            .map(|index| {
                parsed.media_descriptions[index]
                    .attributes
                    .iter()
                    .filter(|value| value.key == "candidate")
                    .filter_map(|value| value.value.clone())
                    .map(|candidate| RTCIceCandidateInit {
                        candidate,
                        ..Default::default()
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(RTCRemoteTransportParameters {
            ice_role: if ice(parsed, selection.first_declared)?.ice_lite {
                RTCIceRole::Controlling
            } else {
                RTCIceRole::Controlled
            },
            ice_parameters: ice(parsed, consumer)?,
            dtls_parameters: RTCDtlsParameters {
                role: remote_role,
                fingerprints,
            },
            local_dtls_role: Some(selection.local_role),
            candidates,
        })
    }
}
