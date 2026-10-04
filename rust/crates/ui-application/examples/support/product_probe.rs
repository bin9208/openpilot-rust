use openpilot_ui_application::context::Context;
use openpilot_ui_framework::widget::{RenderResult, WidgetHandle};

pub struct Probe<'a> {
    pub scene: &'a super::Scene,
    pub context: &'a Context,
    pub widget: &'a WidgetHandle,
}
impl Probe<'_> {
    pub fn flush_ssh(&self) -> Result<(), Box<dyn std::error::Error>> {
        let fetcher = super::product_input::ssh_fetcher(self.widget, self.scene.config.big)?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while fetcher.borrow().is_fetching() {
            if std::time::Instant::now() >= deadline {
                return Err("owned SSH fixture did not finish".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        Ok(())
    }
    pub fn snapshot(
        &self,
        trace: &mut serde_json::Value,
        index: u32,
        rendered: RenderResult,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(root) = &self.scene.root {
            trace["root"] = root.snapshot(self.widget)?;
        } else if self.scene.camera.is_some() {
            trace["camera"] =
                super::product_camera::snapshot(self.context, self.widget, self.scene)?;
        }
        if self.scene.alert.is_some() {
            trace["alert"] =
                super::product_alert::snapshot(self.widget, self.scene.config.big, rendered)?;
        }
        if self.scene.indicator.is_some() {
            trace["indicator"] = super::product_indicator::snapshot(self.widget, &self.scene.kind)?;
        }
        if self.scene.vision.is_some() {
            trace["vision"] = super::product_vision::snapshot(self.context, self.widget)?;
        }
        if self.scene.exp.is_some() {
            trace["exp"] = super::product_exp::snapshot(self.context, self.widget)?;
        }
        if self.scene.hud.is_some() {
            trace["hud"] = super::product_hud::snapshot(self.widget)?;
        }
        if let Some(plot) = &self.scene.plot {
            trace["plot"] =
                super::product_plot::snapshot(self.context, self.widget, plot.step(index)?.mode)?;
        }
        Ok(())
    }
}
