//! Opt-in original aioice request/error transaction behavior for the Rust port.
use super::*;
use stun::error_code::{CODE_BAD_REQUEST, CODE_ROLE_CONFLICT, ErrorCodeAttribute};

pub(super) struct Pending {
    pub local: usize,
    pub remote: usize,
    pub controlling: bool,
}

#[derive(Default)]
pub(super) struct Checks {
    pub pending: HashMap<TransactionId, Pending>,
}

impl Agent {
    /// Enables original aioice check-error semantics for an explicitly owned transport.
    /// Ordinary agents retain the upstream request and response behavior.
    pub fn enable_source_checks(&mut self) {
        self.source_checks.get_or_insert_with(Checks::default);
    }

    pub(super) fn source_inbound(
        &mut self,
        now: Instant,
        m: &mut Message,
        local: usize,
        remote: SocketAddr,
    ) -> Result<bool> {
        if self.source_checks.is_none() {
            return Ok(false);
        }
        if m.contains(ATTR_FINGERPRINT) && FINGERPRINT.check(m).is_err() {
            return Ok(true);
        }
        if m.typ.class == CLASS_ERROR_RESPONSE {
            self.source_error(now, m, local)?;
            return Ok(true);
        }
        if m.typ.class != CLASS_REQUEST {
            return Ok(false);
        }
        let credentials = &self.ufrag_pwd;
        let username_valid = credentials
            .remote_credentials
            .as_ref()
            .is_none_or(|parameters| {
                assert_inbound_username(
                    m,
                    &(credentials.local_credentials.ufrag.clone() + ":" + &parameters.ufrag),
                )
                .is_ok()
            });
        let integrity_valid = !m.contains(ATTR_MESSAGE_INTEGRITY)
            || assert_inbound_message_integrity(
                m,
                credentials.local_credentials.pwd.as_bytes(),
                self.crypto_provider.crypto(),
            )
            .is_ok();
        if m.typ.method != METHOD_BINDING || !username_valid || !integrity_valid {
            self.source_bad_request(now, m, local, remote)?;
            return Ok(true);
        }
        Ok(false)
    }

    fn source_bad_request(
        &mut self,
        now: Instant,
        request: &Message,
        local: usize,
        remote: SocketAddr,
    ) -> Result<()> {
        let mut response = Message::new();
        response.build(&[
            Box::new(MessageType {
                method: request.typ.method,
                class: CLASS_ERROR_RESPONSE,
            }),
            Box::new(request.transaction_id),
            Box::new(CODE_BAD_REQUEST),
            Box::new(MessageIntegrity::new_short_term_integrity_with_provider(
                self.ufrag_pwd.local_credentials.pwd.clone(),
                self.crypto_provider.crypto(),
            )),
            Box::new(FINGERPRINT),
        ])?;
        self.write_outs.push_back(TaggedBytesMut {
            now,
            transport: TransportContext {
                local_addr: self.local_candidates[local].base_addr(),
                peer_addr: remote,
                ecn: None,
                transport_protocol: self.local_candidates[local].network_type().to_protocol(),
            },
            message: BytesMut::from(response.raw.as_slice()),
        });
        Ok(())
    }

    fn source_error(&mut self, now: Instant, message: &Message, local: usize) -> Result<()> {
        let Some(checks) = &mut self.source_checks else {
            return Ok(());
        };
        if !checks
            .pending
            .get(&message.transaction_id)
            .is_some_and(|pending| pending.local == local)
        {
            return Ok(());
        }
        let Some(pending) = checks.pending.remove(&message.transaction_id) else {
            return Ok(());
        };
        let Some(binding) = self.handle_inbound_binding_success(now, message.transaction_id) else {
            return Ok(());
        };
        let Some(index) = self.find_pair(pending.local, pending.remote) else {
            return Ok(());
        };
        if self.candidate_pairs[index].state == CandidatePairState::Failed
            || self.candidate_pairs[index].state == CandidatePairState::Succeeded
                && (self.get_selected_pair() == Some(index) || !binding.is_use_candidate)
        {
            return Ok(());
        }
        if let Some(checks) = &mut self.source_checks {
            let finished: Vec<_> = checks
                .pending
                .iter()
                .filter_map(|(id, check)| {
                    (check.local == pending.local && check.remote == pending.remote).then_some(*id)
                })
                .collect();
            checks.pending.retain(|id, _| !finished.contains(id));
            self.pending_binding_requests
                .retain(|request| !finished.contains(&request.transaction_id));
        }
        let mut code = ErrorCodeAttribute::default();
        if self.nominated_pair == Some(index) {
            self.nominated_pair = None;
        }
        let role_conflict = code.get_from(message).is_ok() && code.code == CODE_ROLE_CONFLICT;
        let pair = &mut self.candidate_pairs[index];
        if role_conflict {
            pair.state = CandidatePairState::Waiting;
            pair.binding_request_count = 0;
            if self.is_controlling == pending.controlling {
                self.switch_role(now);
            }
            self.request_connectivity_check();
        } else {
            pair.state = CandidatePairState::Failed;
            if self.connection_state == ConnectionState::Checking
                && self.get_selected_pair().is_none()
                && !self.candidate_pairs.is_empty()
                && self.candidate_pairs.iter().all(|pair| {
                    pair.state == CandidatePairState::Failed
                        || self.is_controlling && pair.state == CandidatePairState::Succeeded
                })
            {
                self.update_connection_state(Some(now), ConnectionState::Failed);
            }
        }
        Ok(())
    }
}
