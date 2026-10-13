use crate::{
    config::ClusterTheme,
    models::ClusterUiState,
    scene::{ClusterScene, ProfileAdd, SceneBuilder, SceneStateKey},
};
use std::sync::Arc;

struct Entry {
    state: SceneStateKey,
    highlight: bool,
    theme: ClusterTheme,
    scene: Arc<ClusterScene>,
}
#[derive(Default)]
pub(super) struct SceneCache {
    builder: SceneBuilder,
    entry: Option<Entry>,
}
impl SceneCache {
    pub fn get(
        &mut self,
        state: &ClusterUiState,
        highlight: bool,
        theme: &ClusterTheme,
        profile: ProfileAdd<'_>,
    ) -> Arc<ClusterScene> {
        if let Some(entry) = self.entry.as_ref().filter(|entry| {
            entry.highlight == highlight && entry.theme == *theme && entry.state.matches(state)
        }) {
            return Arc::clone(&entry.scene);
        }
        let scene = Arc::new(self.builder.build(state, highlight, theme, profile));
        self.entry = Some(Entry {
            state: SceneStateKey::from_state(state),
            highlight,
            theme: theme.clone(),
            scene: Arc::clone(&scene),
        });
        scene
    }
    pub fn clear(&mut self) {
        self.entry = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scene_is_reused_when_only_hud_data_changes() {
        let mut cache = SceneCache::default();
        let mut state = ClusterUiState::default();
        let theme = crate::config::light_theme();
        let original = cache.get(&state, false, &theme, None);
        state.network_address = Some("192.0.2.253".into());
        let next = cache.get(&state, false, &theme, None);
        assert!(Arc::ptr_eq(&original, &next));
    }
    #[test]
    fn scene_is_rebuilt_when_highlight_changes() {
        let mut cache = SceneCache::default();
        let state = ClusterUiState::default();
        let theme = crate::config::light_theme();
        let original = cache.get(&state, false, &theme, None);
        let next = cache.get(&state, true, &theme, None);
        assert!(!Arc::ptr_eq(&original, &next));
    }
}
