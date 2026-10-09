use super::super::{events::ErrorEvent, Failure, Pull};
use crate::{
    auto_update::{AutoRebootCondition, RebootSample, DISENGAGED_DELAY},
    Error, Value,
};

pub struct Reboot {
    mode: String,
    head: String,
    condition: AutoRebootCondition,
}

impl Reboot {
    pub fn begin(pull: &Pull, mode: &str, head: &str) -> Result<Option<Self>, Failure> {
        let reboot = Self {
            mode: mode.into(),
            head: head.into(),
            condition: AutoRebootCondition::new(mode, DISENGAGED_DELAY),
        };
        let mut fields = reboot.fields();
        crate::json_fields::set(&mut fields, "error_code", Value::text(""))?;
        crate::json_fields::set(&mut fields, "error", Value::text(""))?;
        if !pull.event("reboot_pending", &fields)? {
            Self::write_failed(pull, "Unable to save automatic-update reboot state")?;
            return Ok(None);
        }
        println!("[auto_update] reboot armed mode={mode}");
        Ok(Some(reboot))
    }

    fn fields(&self) -> Value {
        Value::object([
            ("new_head", Value::text(&self.head)),
            ("target_head", Value::text(&self.head)),
            ("reboot_mode", Value::text(&self.mode)),
        ])
    }

    pub fn select(&mut self, pull: &Pull, selected: &str) -> Result<bool, Failure> {
        if selected == "off" {
            self.mode = selected.into();
            pull.event("updated", &self.fields())?;
            println!("[auto_update] reboot cancelled");
            return Ok(true);
        }
        if selected != self.mode {
            self.mode = selected.into();
            self.condition = AutoRebootCondition::new(selected, DISENGAGED_DELAY);
            println!("[auto_update] reboot mode changed mode={selected}");
        }
        Ok(false)
    }

    pub fn sample(
        &mut self,
        pull: &Pull,
        sample: &RebootSample<'_>,
        request: impl FnOnce() -> Result<(), Error>,
    ) -> Result<bool, Failure> {
        if !self.condition.update(sample) {
            return Ok(false);
        }
        let mut fields = self.fields();
        crate::json_fields::set(
            &mut fields,
            "reboot_requested_head",
            Value::text(&self.head),
        )?;
        if pull
            .store
            .auto_update()
            .get("reboot_requested_head")
            .text_eq(&self.head)
        {
            pull.error(
                "duplicate_reboot_blocked",
                ErrorEvent {
                    detail: format!(
                        "Automatic reboot for {} was already requested",
                        super::super::head_prefix(&self.head)
                    ),
                    blocked: true,
                    fields,
                },
            )?;
            return Ok(true);
        }
        crate::json_fields::set(&mut fields, "error_code", Value::text(""))?;
        crate::json_fields::set(&mut fields, "error", Value::text(""))?;
        if !pull.event("reboot_requested", &fields)? {
            Self::write_failed(pull, "Unable to save automatic-update reboot request")?;
            return Ok(true);
        }
        println!("[auto_update] reboot condition met mode={}", self.mode);
        request()?;
        Ok(true)
    }

    fn write_failed(pull: &Pull, detail: &str) -> Result<(), Failure> {
        pull.error(
            "state_write_failed",
            ErrorEvent {
                detail: detail.into(),
                blocked: false,
                fields: Value::object([]),
            },
        )
    }
}
