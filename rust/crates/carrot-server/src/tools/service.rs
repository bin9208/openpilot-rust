use super::{
    admission::Counter,
    config::Config,
    context::{Context, Reply},
    dispatch,
    jobs::Store,
    policy::Action,
    runner::{Failure, Runner},
};
use crate::{json_fields::set, Error, Value};
use std::sync::{Arc, Mutex};
use tokio::{
    sync::{oneshot, watch},
    task::JoinHandle,
};

pub struct Service {
    pub jobs: Arc<Store>,
    pub config: Arc<Config>,
    tasks: Mutex<Vec<JoinHandle<()>>>,
    stop: watch::Sender<bool>,
    count: Arc<Counter>,
}
impl Service {
    pub fn new(config: Config) -> Arc<Self> {
        let (stop, _) = watch::channel(false);
        Arc::new(Self {
            jobs: Store::new(config.paths.history.clone()),
            config: Arc::new(config),
            tasks: Mutex::new(Vec::new()),
            stop,
            count: Counter::new(),
        })
    }
    fn context(&self, id: Option<String>) -> Context {
        Context {
            config: Arc::clone(&self.config),
            jobs: Arc::clone(&self.jobs),
            id,
            runner: Runner {
                repository: self.config.paths.repository.clone(),
                launcher: self.config.paths.launcher.clone(),
                lock: None,
                stopped: self.stop.subscribe(),
            },
        }
    }
    pub fn start(self: &Arc<Self>, body: Value) -> Result<Reply, Error> {
        if let Err(value) = Action::parse(body.get("action")) {
            return Ok(Reply { status: 400, value });
        }
        let _admission = self.count.admit()?;
        let action = body.get("action").py_string()?;
        let id = self.jobs.create(action, body.clone(), None)?;
        let context = self.context(Some(id.clone()));
        self.register(tokio::spawn(async move {
            execute(context, body).await;
        }))?;
        Ok(Reply::ok(Value::object([
            ("ok", Value::Bool(true)),
            ("job_id", Value::text(&id)),
            ("status", Value::text("running")),
        ])))
    }
    pub async fn sync(self: &Arc<Self>, body: Value) -> Result<Reply, Error> {
        let admission = self.count.admit()?;
        let context = self.context(None);
        let (reply, response) = oneshot::channel();
        self.register(tokio::spawn(async move {
            let _owned = admission;
            let result = execute(context, body).await;
            let _sent = reply.send(result);
        }))?;
        response
            .await
            .map_err(|_| Error::Source("Tools service unavailable".into()))
    }
    fn register(&self, task: JoinHandle<()>) -> Result<(), Error> {
        let mut tasks = self
            .tasks
            .lock()
            .map_err(|_| Error::Source("Tools owner poisoned".into()))?;
        tasks.retain(|task| !task.is_finished());
        tasks.push(task);
        Ok(())
    }
    pub fn quiesce(&self) {
        self.count.quiesce();
    }
    pub fn is_idle(&self) -> bool {
        self.count.idle()
    }
    pub fn changed(&self) -> watch::Receiver<()> {
        self.count.changed()
    }
    pub fn force(&self) {
        self.quiesce();
        self.stop.send_replace(true);
    }
    pub async fn shutdown(&self) -> Result<(), Error> {
        self.force();
        let tasks = std::mem::take(
            &mut *self
                .tasks
                .lock()
                .map_err(|_| Error::Source("Tools owner poisoned".into()))?,
        );
        let mut failure = None;
        for task in tasks {
            if let Err(error) = task.await {
                failure.get_or_insert_with(|| Error::Source(error.to_string()));
            }
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.force();
        if let Ok(tasks) = self.tasks.get_mut() {
            for task in tasks {
                task.abort();
            }
        }
    }
}
async fn execute(mut context: Context, body: Value) -> Reply {
    let result = dispatch::run(&mut context, &body).await;
    let reply = match result {
        Ok(reply) => reply,
        Err(Failure::Cancelled) => return Reply::error(503, "Tools operation cancelled"),
        Err(Failure::Timeout) => {
            let mut reply = Reply::error(if context.streaming() { 200 } else { 504 }, "timeout");
            if context.streaming() {
                if let Err(error) = set(&mut reply.value, "error_code", Value::text("CMD_TIMEOUT"))
                {
                    eprintln!("Tools timeout: {error}");
                }
            }
            reply
        }
        Err(Failure::Boundary(error)) => {
            if let Err(log) = context.append(&format!("\nTools action failed: {error}\n")) {
                eprintln!("Tools log: {log}");
            }
            Reply::error(500, &error.to_string())
        }
    };
    if let Some(id) = &context.id {
        let running = context
            .jobs
            .get(id)
            .ok()
            .flatten()
            .is_some_and(|job| job.get("status").text_eq("running"));
        if running {
            if let Err(error) =
                context
                    .jobs
                    .finish(id, reply.value.get("ok").truth(), reply.value.clone())
            {
                eprintln!("Tools job finish: {error}");
            }
        }
    }
    reply
}
