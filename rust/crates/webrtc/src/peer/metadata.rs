use crate::{network::Gathered, Error};
use rtc::{
    peer_connection::{
        certificate::RTCCertificate,
        configuration::{media_engine::MediaEngine, RTCConfigurationBuilder},
        RTCPeerConnection, RTCPeerConnectionBuilder,
    },
    rtp_transceiver::rtp_sender::{
        RTCPFeedback, RTCRtpCodec, RTCRtpCodecParameters, RTCRtpHeaderExtensionCapability,
        RtpCodecKind,
    },
    sdp::description::{
        common::{Address, Attribute, ConnectionInformation},
        session::SessionDescription,
    },
};
use std::{net::UdpSocket, time::Instant};

pub(super) fn source_cname(answer: &mut SessionDescription, cname: &str) {
    for media in &mut answer.media_descriptions {
        for attribute in &mut media.attributes {
            if attribute.key == "ssrc" {
                if let Some((ssrc, value)) = attribute
                    .value
                    .as_deref()
                    .and_then(|value| value.split_once(' '))
                {
                    if value.starts_with("cname:") {
                        attribute.value = Some(format!("{ssrc} cname:{cname}"));
                    }
                }
            }
        }
    }
}

fn video_codec(name: &str, parameters: &str) -> RTCRtpCodec {
    RTCRtpCodec {
        mime_type: format!("video/{name}"),
        clock_rate: 90_000,
        channels: 0,
        sdp_fmtp_line: parameters.to_owned(),
        rtcp_feedback: [("nack", ""), ("nack", "pli"), ("goog-remb", "")]
            .into_iter()
            .map(|(typ, parameter)| RTCPFeedback {
                typ: typ.to_owned(),
                parameter: parameter.to_owned(),
            })
            .collect(),
    }
}

pub(super) fn h264(profile: &str) -> RTCRtpCodec {
    video_codec(
        "H264",
        &format!("level-asymmetry-allowed=1;packetization-mode=1;profile-level-id={profile}"),
    )
}

pub(super) fn configured(certificate: RTCCertificate) -> Result<RTCPeerConnection, Error> {
    let mut media = MediaEngine::default();
    for (name, clock_rate, channels, payload_type) in [
        ("opus", 48_000, 2, 96),
        ("G722", 8000, 1, 9),
        ("PCMU", 8000, 1, 0),
        ("PCMA", 8000, 1, 8),
    ] {
        media.register_codec(
            RTCRtpCodecParameters {
                rtp_codec: RTCRtpCodec {
                    mime_type: format!("audio/{name}"),
                    clock_rate,
                    channels,
                    ..Default::default()
                },
                payload_type,
            },
            RtpCodecKind::Audio,
        )?;
    }
    for (codec, payload_type) in [
        (video_codec("VP8", ""), 97),
        (h264("42001f"), 99),
        (h264("42e01f"), 101),
    ] {
        media.register_codec(
            RTCRtpCodecParameters {
                rtp_codec: codec,
                payload_type,
            },
            RtpCodecKind::Video,
        )?;
        media.register_codec(
            RTCRtpCodecParameters {
                rtp_codec: RTCRtpCodec {
                    mime_type: "video/rtx".to_owned(),
                    clock_rate: 90_000,
                    sdp_fmtp_line: format!("apt={payload_type}"),
                    ..Default::default()
                },
                payload_type: payload_type + 1,
            },
            RtpCodecKind::Video,
        )?;
    }
    for uri in [
        "urn:ietf:params:rtp-hdrext:sdes:mid",
        "urn:ietf:params:rtp-hdrext:ssrc-audio-level",
    ] {
        media.register_header_extension(
            RTCRtpHeaderExtensionCapability {
                uri: uri.to_owned(),
            },
            RtpCodecKind::Audio,
            None,
        )?;
    }
    for uri in [
        "urn:ietf:params:rtp-hdrext:sdes:mid",
        "http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time",
    ] {
        media.register_header_extension(
            RTCRtpHeaderExtensionCapability {
                uri: uri.to_owned(),
            },
            RtpCodecKind::Video,
            None,
        )?;
    }
    Ok(RTCPeerConnectionBuilder::new()
        .with_interceptor_registry(rtc::interceptor::Registry::new().with(
            rtc::interceptor::Slot::Custom(usize::MAX),
            super::feedback::Feedback::default(),
        ))
        .with_media_engine(media)
        .with_configuration(
            RTCConfigurationBuilder::new()
                .with_certificates(vec![certificate])
                .build(),
        )
        .build(Instant::now())?)
}

pub(super) fn projected(
    answer: &mut SessionDescription,
    gathered: Option<&Gathered>,
) -> Result<(), Error> {
    let address = gathered
        .and_then(|gathered| gathered.sockets.first())
        .map(UdpSocket::local_addr)
        .transpose()?;
    for media in &mut answer.media_descriptions {
        media
            .attributes
            .retain(|value| !matches!(value.key.as_str(), "candidate" | "end-of-candidates"));
        media.media_name.port.value =
            address.map_or(Ok(9), |address| isize::try_from(address.port()))?;
        media.connection_information = Some(ConnectionInformation {
            network_type: "IN".to_owned(),
            address_type: if address.is_some_and(|address| address.is_ipv6()) {
                "IP6"
            } else {
                "IP4"
            }
            .to_owned(),
            address: Some(Address {
                address: address
                    .map_or_else(|| "0.0.0.0".to_owned(), |address| address.ip().to_string()),
                ..Default::default()
            }),
        });
        if let Some(gathered) = gathered {
            for candidate in &gathered.candidates {
                media.attributes.push(Attribute::new(
                    "candidate".to_owned(),
                    Some(
                        candidate
                            .candidate
                            .strip_prefix("candidate:")
                            .unwrap_or(&candidate.candidate)
                            .to_owned(),
                    ),
                ));
            }
            media
                .attributes
                .push(Attribute::new("end-of-candidates".to_owned(), None));
        }
    }
    Ok(())
}
