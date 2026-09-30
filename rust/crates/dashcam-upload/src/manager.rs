use crate::{
    catalog,
    state::{Job, Jobs},
    worker::{self, Event, Packet, Request, Settings},
    Error,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
};

struct Running {
    child: Child,
    control: ChildStdin,
}
impl Drop for Running {
    fn drop(&mut self) {
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            if let Err(error) = self.child.kill() {
                eprintln!("dashcam upload worker termination: {error}");
            }
            if let Err(error) = self.child.wait() {
                eprintln!("dashcam upload worker wait: {error}");
            }
        }
    }
}
pub struct Manager {
    jobs: Arc<Mutex<Jobs>>,
    running: Arc<Mutex<HashMap<String, Running>>>,
    executable: PathBuf,
}
fn lock<T>(mutex: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>, Error> {
    mutex
        .lock()
        .map_err(|error| Error::Runtime(error.to_string()))
}
fn receive(jobs: &Mutex<Jobs>, id: &str, packet: Packet) -> Result<(), Error> {
    let mut jobs = lock(jobs)?;
    let Some(job) = jobs.get_mut(id) else {
        return Ok(());
    };
    if job.status != "running" {
        return Ok(());
    }
    match packet.event {
        Event::Touch => job.touch(packet.clock),
        Event::Append { text } => job.append(Some(&text), packet.clock),
        Event::Progress { patch } => job.update(patch, packet.clock)?,
        Event::Context {
            metadata,
            remote_base_path,
        } => {
            job.upload_meta = metadata;
            job.remote_base_path = remote_base_path;
        }
        Event::Partial { results } => job.partial_results = results,
        Event::Finish { patch } => {
            job.finish(patch, packet.clock);
            jobs.prune();
        }
    }
    Ok(())
}
impl Manager {
    pub fn new(executable: PathBuf) -> Self {
        Self {
            jobs: Arc::new(Mutex::new(Jobs::default())),
            running: Arc::new(Mutex::new(HashMap::new())),
            executable,
        }
    }
    pub fn start(
        &mut self,
        root: &Path,
        segments: &[String],
        settings: Option<Settings>,
    ) -> Result<Value, Error> {
        let segments = catalog::validate_selection(root, segments)?;
        self.expire(None)?;
        let mut jobs = lock(&self.jobs)?;
        if let Some(job) = jobs.0.iter().find(|job| job.status == "running") {
            return Ok(
                json!({"ok":false,"error":"upload already running","job_id":job.id,"job":job.snapshot()?}),
            );
        }
        let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_owned();
        jobs.create(id.clone(), segments.clone(), worker::clock());
        drop(jobs);
        let spawn = (|| -> Result<(), Error> {
            let mut child = Command::new(&self.executable)
                .arg("--worker")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()?;
            let mut control = child
                .stdin
                .take()
                .ok_or_else(|| Error::Runtime("worker control pipe missing".into()))?;
            let output = child
                .stdout
                .take()
                .ok_or_else(|| Error::Runtime("worker output pipe missing".into()))?;
            if let Err(error) = serde_json::to_writer(
                &mut control,
                &Request {
                    parent_pid: std::process::id(),
                    root: root.to_owned(),
                    id: id.clone(),
                    segments,
                    settings,
                },
            )
            .map_err(Error::from)
            .and_then(|_| {
                control.write_all(b"\n")?;
                control.flush()?;
                Ok(())
            }) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
            lock(&self.running)?.insert(id.clone(), Running { child, control });
            let (jobs, job_id) = (self.jobs.clone(), id.clone());
            let running = self.running.clone();
            thread::spawn(move || {
                let result = (|| -> Result<(), Error> {
                    for line in BufReader::new(output).lines() {
                        receive(&jobs, &job_id, serde_json::from_str(&line?)?)?;
                    }
                    Ok(())
                })();
                if let Ok(mut jobs) = jobs.lock() {
                    if let Some(job) = jobs.get_mut(&job_id) {
                        job.fail_running(
                            &result.err().map_or_else(
                                || "upload task ended without a final state".into(),
                                |error| error.to_string(),
                            ),
                            worker::clock(),
                        );
                    }
                    jobs.prune();
                }
                let ended = running
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .remove(&job_id);
                drop(ended);
            });
            Ok(())
        })();
        if let Err(error) = spawn {
            if let Some(job) = lock(&self.jobs)?.get_mut(&id) {
                job.fail_running(&error.to_string(), worker::clock());
            }
            return Err(error);
        }
        Ok(json!({"ok":true,"job_id":id,"status":"running"}))
    }
    pub fn snapshot(&mut self, id: &str) -> Result<Value, Error> {
        self.expire(None)?;
        lock(&self.jobs)?.get_mut(id).map_or_else(
            || Ok(json!({"ok":false,"error":"job not found"})),
            |job| job.snapshot().map_err(Error::from),
        )
    }
    pub fn cancel(&mut self, id: &str) -> Result<Value, Error> {
        let response = lock(&self.jobs)?.get_mut(id).map_or_else(
            || Ok(json!({"ok":false,"error":"job not found"})),
            |job| job.cancel(worker::clock()).map_err(Error::from),
        )?;
        if response["ok"] == true && response.get("already_done").is_none() {
            if let Some(running) = lock(&self.running)?.get_mut(id) {
                let _ = running.control.write_all(b"cancel\n");
                let _ = running.control.flush();
            }
        }
        Ok(response)
    }
    pub fn expire(&mut self, now: Option<f64>) -> Result<(), Error> {
        let mut jobs = lock(&self.jobs)?;
        let before = jobs
            .0
            .iter()
            .filter(|job| job.status == "running")
            .map(|job| job.id.clone())
            .collect::<Vec<_>>();
        jobs.expire(now, worker::clock());
        let mut running = lock(&self.running)?;
        for id in before {
            if jobs.get_mut(&id).is_some_and(|job| job.status != "running") {
                running.remove(&id);
            }
        }
        let mut ended = Vec::new();
        for (id, running) in running.iter_mut() {
            if running.child.try_wait()?.is_some() {
                ended.push(id.clone());
            }
        }
        for id in ended {
            running.remove(&id);
        }
        Ok(())
    }
    pub fn jobs(&self) -> Result<Vec<Job>, Error> {
        Ok(lock(&self.jobs)?.0.clone())
    }
}
impl Drop for Manager {
    fn drop(&mut self) {
        let mut running = self
            .running
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let owned = std::mem::take(&mut *running);
        drop(running);
        drop(owned);
    }
}
