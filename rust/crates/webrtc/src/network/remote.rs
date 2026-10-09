use super::{Lease, Network, dns::Name};
use crate::Error;
use rtc::{
    ice::candidate::unmarshal_candidate,
    peer_connection::{RTCRemoteTransportParameters, transport::RTCIceCandidateInit},
};
use tokio::task::JoinSet;

pub(crate) struct Remote {
    pub parameters: RTCRemoteTransportParameters,
    pub lease: Option<Lease>,
}

async fn candidate(
    mut candidate: RTCIceCandidateInit,
    host: &str,
    resolver: Lease,
) -> Result<Option<RTCIceCandidateInit>, Error> {
    let Some(address) = resolver.resolve(host).await? else {
        eprintln!("WebRTC remote candidate {host} could not be resolved");
        return Ok(None);
    };
    let raw = candidate
        .candidate
        .strip_prefix("candidate:")
        .unwrap_or(&candidate.candidate);
    candidate.candidate = raw
        .split_whitespace()
        .enumerate()
        .map(|(index, value)| {
            if index == 4 {
                address.to_string()
            } else {
                value.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    Ok(Some(candidate))
}

impl Network {
    pub(crate) async fn resolve(
        &self,
        mut parameters: RTCRemoteTransportParameters,
    ) -> Result<Remote, Error> {
        let mut lease = None;
        let mut candidates = Vec::new();
        let mut queries = JoinSet::new();
        for remote in std::mem::take(&mut parameters.candidates) {
            let raw = remote
                .candidate
                .strip_prefix("candidate:")
                .unwrap_or(&remote.candidate);
            let parsed = unmarshal_candidate(raw)?;
            let host = parsed.address();
            if Name::hostname(host).is_some() {
                if lease.is_none() {
                    lease = Some(self.mdns.acquire()?);
                }
                let resolver = self.mdns.acquire()?;
                let host = host.to_owned();
                queries.spawn(async move { candidate(remote, &host, resolver).await });
            } else {
                candidates.push(remote);
            }
        }
        while let Some(result) = queries.join_next().await {
            if let Some(candidate) =
                result.map_err(|_| Error::Contract("mDNS candidate task failed"))??
            {
                candidates.push(candidate);
            }
        }
        parameters.candidates = candidates;
        Ok(Remote { parameters, lease })
    }
}
