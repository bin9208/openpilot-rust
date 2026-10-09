use super::{bridge::Bridge, media_stats::age, wire};
use crate::Value;
use std::time::Duration;

impl Bridge {
    pub fn status(&mut self) -> Value {
        let allowed = self.allowed(true);
        self.diagnostics
            .retain(|_, (at, _)| at.elapsed() <= Duration::from_secs(15));
        let state = self.last_state.as_ref().unwrap_or(&Value::Null);
        let navigation = state.get("navigationStatus");
        let media = self.pipeline.status();
        let counts = self.clients.counts();
        let diagnostics = self
            .diagnostics
            .iter()
            .map(|(peer, (at, value))| {
                let mut value = value.clone();
                wire::update(
                    &mut value,
                    [("peer", Value::text(peer)), ("ageMs", age(Some(*at)))],
                );
                value
            })
            .collect();
        Value::object([
            ("stateClients", Value::integer(counts.0)),
            ("mediaClients", Value::integer(counts.1)),
            ("streamAllowed", Value::Bool(allowed)),
            ("readerActive", Value::Bool(self.running)),
            ("connected", Value::Bool(state.get("connected").truth())),
            (
                "guidanceActive",
                Value::Bool(navigation.get("guidanceActive").truth()),
            ),
            (
                "routePresent",
                Value::Bool(navigation.get("routePresent").truth()),
            ),
            (
                "stateFresh",
                Value::Bool(
                    self.state_at
                        .is_some_and(|at| at.elapsed().as_millis() <= 3000),
                ),
            ),
            ("mapFresh", media.get("mapFresh").clone()),
            ("stateAgeMs", age(self.state_at)),
            ("mapAgeMs", media.get("mapAgeMs").clone()),
            ("stateMessages", Value::integer(self.state_count)),
            ("mediaMessages", Value::integer(self.media_count)),
            ("mapStream", media.get("mapStream").clone()),
            ("hudMapProfile", Value::Bool(self.hud_clients > 0)),
            ("clientDiagnostics", Value::Array(diagnostics)),
            ("error", Value::text(&self.error)),
        ])
    }
}
