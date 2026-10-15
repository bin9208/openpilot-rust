use super::super::inputs::Decoded;
use super::super::{views::Views, Output, Planner};
use crate::{lateral_planner, publication};
use crate::{parameters::Parameters, platform::Clock, Error};

impl Planner {
    pub(super) fn lateral_step(
        &mut self,
        views: &Views<'_>,
        parameters: &mut impl Parameters,
        clock: &impl Clock,
        output: &mut impl Output,
    ) -> Result<[f64; 2], Error> {
        let state = views.0;
        let model_topic = state.topic("modelV2")?;
        let started = clock.monotonic();
        let model_age =
            (started - crate::number::timestamp_seconds(model_topic.log_mono_time)?) * 1000.;
        self.model_frame += 1;
        let decoded = Decoded::read(views)?;
        let warning = self.lateral.update(
            lateral_planner::Input {
                car: &decoded.car,
                model: &decoded.model,
                curvature: f64::from(views.controls()?.get_curvature()),
                curve_speed: decoded.curve_speed,
                atc_active: self.carrot.atc_active,
            },
            parameters,
            &mut || clock.monotonic(),
        )?;
        if warning {
            output.warning("Lateral mpc - nan: True".into())?;
        }
        let bytes = publication::lateral(
            &self.lateral,
            &decoded.model.meta,
            clock.message_time()?,
            model_topic.log_mono_time,
            state.all_checks(&["carState", "controlsState", "modelV2"])?,
            false,
        )?;
        output.send("lateralPlan", &bytes)?;
        let warning = self.departure.update_model(
            self.model_frame,
            &decoded.model,
            &decoded.car,
            views.control()?.get_lat_active(),
        )?;
        output.send(
            "driverAssistance",
            &publication::assistance(
                warning,
                clock.message_time()?,
                state.all_checks(&["carState", "carControl", "modelV2", "liveParameters"])?,
            ),
        )?;
        Ok([model_age, (clock.monotonic() - started) * 1000.])
    }
}
