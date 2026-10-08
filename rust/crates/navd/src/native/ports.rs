use super::{diagnostics, http, publisher::Publisher, stop::Stop, timestamp};
use crate::{
    geometry::Coordinate,
    route::{Diagnostic, Instruction, Ports, RequestError},
    wire, Error,
};
use openpilot_logging::producer::Logger;
use openpilot_params::Params;
use std::{
    sync::{atomic::Ordering, Arc},
    thread,
};

pub struct NativePorts<'a> {
    pub params: Params,
    pub logger: Logger,
    pub publisher: Publisher,
    pub stop: &'a Stop,
}

impl NativePorts<'_> {
    fn response(&self, url: &str) -> Result<http::Response, RequestError> {
        let (sender, receiver) = crossbeam_channel::bounded(1);
        let url = url.to_owned();
        let stop = Arc::clone(&self.stop.requested);
        let worker = thread::Builder::new()
            .name("navd-http".into())
            .spawn(move || {
                if sender.send(http::get(&url)).is_err() && !stop.load(Ordering::Acquire) {
                    eprintln!("navd HTTP result receiver closed unexpectedly");
                }
            })
            .map_err(|error| RequestError::Transport(error.to_string()))?;
        match self.stop.receive(&receiver) {
            Ok(result) => {
                worker
                    .join()
                    .map_err(|_| RequestError::Transport("HTTP worker panicked".into()))?;
                result
            }
            Err(Error::Interrupted) => Err(RequestError::Interrupted),
            Err(error) => Err(RequestError::Transport(error.to_string())),
        }
    }
}

impl Ports for NativePorts<'_> {
    fn parameter(&mut self, name: &str) -> Result<Option<String>, Error> {
        self.stop.check()?;
        Ok(openpilot_params_typed::get_string(
            &self.params,
            name,
            &mut self.logger,
        )?)
    }

    fn remove_parameter(&mut self, name: &str) -> Result<(), Error> {
        self.stop.check()?;
        super::parameters::write_status(self.params.remove(name))
    }

    fn request(&mut self, url: &str) -> Result<serde_json::Value, RequestError> {
        let response = self.response(url)?;
        if response.status != 200 {
            diagnostics::api_failure(&mut self.logger, response.status, &response.text);
        }
        if response.status >= 400 {
            return Err(RequestError::Status {
                status: response.status,
                body: response.text,
            });
        }
        response.json
    }

    fn instruction(&mut self, message: &Instruction) -> Result<(), Error> {
        self.stop.check()?;
        self.publisher
            .send("navInstruction", wire::instruction(message, timestamp()?)?)
    }

    fn route(&mut self, coordinates: &[Coordinate]) -> Result<(), Error> {
        self.stop.check()?;
        self.publisher
            .send("navRouteNavd", wire::route(coordinates, timestamp()?)?)
    }

    fn diagnostic(&mut self, event: Diagnostic) {
        diagnostics::event(&mut self.logger, event);
    }

    fn geometry_changed(
        &mut self,
        coordinates: Result<Vec<Coordinate>, Error>,
    ) -> Result<(), Error> {
        self.stop.check()?;
        self.publisher.geometry(coordinates)
    }
}
