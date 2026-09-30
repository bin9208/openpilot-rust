//! Source local/global merge precedence and unwind-safe scoped context.
use crate::{Error, Fields, Value};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, RwLock},
};

#[derive(Clone)]
pub struct GlobalContext(Arc<RwLock<Fields>>);
impl Default for GlobalContext {
    fn default() -> Self {
        let mut context = Fields::new();
        context.insert("runtime_language".into(), Value::Text("rust".into()));
        context.insert(
            "source_commit".into(),
            Value::Text(env!("OPENPILOT_LOGGING_SOURCE_COMMIT").into()),
        );
        context.insert(
            "source_tree".into(),
            Value::Text(env!("OPENPILOT_LOGGING_SOURCE_TREE").into()),
        );
        Self(Arc::new(RwLock::new(context)))
    }
}
impl GlobalContext {
    pub fn bind(&self, fields: Fields) -> Result<(), Error> {
        self.0
            .write()
            .map_err(|_| Error::Contract("global logging context lock poisoned"))?
            .extend(fields.iter().map(|(k, v)| (k.clone(), v.clone())));
        Ok(())
    }
    pub fn local(&self) -> LocalContext {
        LocalContext {
            global: self.clone(),
            local: Rc::new(RefCell::new(Fields::new())),
        }
    }
}
pub struct LocalContext {
    global: GlobalContext,
    local: Rc<RefCell<Fields>>,
}
impl LocalContext {
    pub fn bind(&self, fields: Fields) {
        self.local
            .borrow_mut()
            .extend(fields.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
    pub fn snapshot(&self) -> Result<Fields, Error> {
        let mut fields = self.local.borrow().clone();
        fields.extend(
            self.global
                .0
                .read()
                .map_err(|_| Error::Contract("global logging context lock poisoned"))?
                .iter()
                .map(|(k, v)| (k.clone(), v.clone())),
        );
        Ok(fields)
    }
    pub fn scoped(&self, fields: Fields) -> ContextGuard {
        let previous = self.local.borrow().clone();
        self.bind(fields);
        ContextGuard {
            local: Rc::clone(&self.local),
            previous: Some(previous),
        }
    }
}
#[must_use = "retain the context guard until the scoped operation finishes"]
pub struct ContextGuard {
    local: Rc<RefCell<Fields>>,
    previous: Option<Fields>,
}
impl Drop for ContextGuard {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            *self.local.borrow_mut() = previous;
        }
    }
}
