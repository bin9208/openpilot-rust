use super::{paths, preload, Error, Reader, Segment, Source, Timeline};
use crate::route_replay::video;
use std::path::Path;

impl Source {
    pub(super) fn ensure_loaded(&mut self, time: f64) -> Result<(), Error> {
        if self.timeline.first_time().is_some_and(|first| time < first) && !self.end {
            self.reset()?;
        }
        if self.active.is_some() && !self.timeline.is_empty() && time >= self.duration() - 20.0 {
            self.finish_preload(false)?;
        }
        while !self.end
            && (self.timeline.is_empty()
                || self.timeline.chunk_count() < 1
                || time >= self.duration() - 5.0)
        {
            if !self.load_next()? {
                break;
            }
        }
        self.timeline.trim(time);
        if !self.end && self.active.is_none() && self.next < self.files.len() {
            self.request_next()?;
        }
        Ok(())
    }
    fn load_next(&mut self) -> Result<bool, Error> {
        if self.active.is_some() {
            return self.finish_preload(true);
        }
        if self.next >= self.files.len() {
            self.end = true;
            return Ok(false);
        }
        self.request_next()?;
        self.finish_preload(true)
    }
    fn request_next(&mut self) -> Result<(), Error> {
        let request = preload::ParseRequest {
            generation: self.generation,
            index: self.next,
            path: self.files[self.next].clone(),
        };
        self.next += 1;
        self.active = Some(request.clone());
        self.worker.request(request)
    }
    fn finish_preload(&mut self, block: bool) -> Result<bool, Error> {
        if self.active.is_none() {
            return Ok(false);
        }
        let Some(result) = self.worker.receive(block)? else {
            return Ok(false);
        };
        let active = self.active.take();
        if result.generation != self.generation {
            return Ok(false);
        }
        let frames = result.result.map_err(|error| {
            Error::Invalid(format!(
                "failed to preload route log {}:\n{error}",
                active.as_ref().map_or_else(
                    || "None".into(),
                    |request| request.path.display().to_string()
                )
            ))
        })?;
        self.loaded = self.loaded.max(self.next);
        if let (Some(first), Some(last)) = (frames.first(), frames.last()) {
            self.append_video(&result.path, first.t, last.t);
        }
        self.timeline.append(result.index, result.path, frames);
        Ok(true)
    }
    fn append_video(&mut self, path: &Path, start_t: f64, end_t: f64) {
        for (index, name) in ["road", "wide"].into_iter().enumerate() {
            if let Some(video) =
                video::path_for_stream(path.parent().unwrap_or(Path::new(".")), name)
            {
                self.streams[index].push(Segment {
                    index: paths::segment_index(path),
                    path: video,
                    start_t,
                    end_t,
                });
            }
        }
        self.reader.set_segments(self.streams[self.stream].clone());
    }
    fn reset(&mut self) -> Result<(), Error> {
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| Error::Invalid("route preload generation overflow".into()))?;
        if self.active.is_some() {
            self.finish_preload(true)?;
        }
        self.active = None;
        self.worker.restart()?;
        self.reader.close()?;
        self.timeline = Timeline::default();
        self.streams = [Vec::new(), Vec::new()];
        self.stream = 0;
        self.reader = Reader::new(Vec::new(), "road", self.config.video.clone());
        self.next = 0;
        self.loaded = 0;
        self.end = false;
        Ok(())
    }
}
