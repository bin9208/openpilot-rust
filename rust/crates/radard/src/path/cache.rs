use super::{geometry::Geometry, Projection};
use crate::Error;
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Deserialize, Serialize)]
pub struct GeometryEntry {
    path: Vec<[f64; 2]>,
    value: Geometry,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct ProjectionEntry {
    path: Vec<[f64; 2]>,
    x: f64,
    y: f64,
    value: Projection,
}

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct Snapshot {
    pub geometry: Vec<GeometryEntry>,
    pub projection: Vec<ProjectionEntry>,
}

#[derive(Default)]
struct Cache {
    geometry: Vec<(Vec<[f64; 2]>, Rc<Geometry>)>,
    projection: Vec<ProjectionEntry>,
}

// The source module LRU spans owners and equates signed-zero keys while
// retaining the first computed value. The serial daemon has one policy thread.
thread_local! {
    static CACHE: RefCell<Cache> = const { RefCell::new(Cache { geometry: Vec::new(), projection: Vec::new() }) };
}

impl Cache {
    fn geometry(&mut self, path: &[[f64; 2]]) -> Rc<Geometry> {
        if let Some(index) = self.geometry.iter().position(|(key, _)| key == path) {
            let entry = self.geometry.remove(index);
            let value = Rc::clone(&entry.1);
            self.geometry.push(entry);
            return value;
        }
        let value = Rc::new(Geometry::new(path));
        if self.geometry.len() == 8 {
            self.geometry.remove(0);
        }
        self.geometry.push((path.to_vec(), Rc::clone(&value)));
        value
    }

    fn project(&mut self, path: &[[f64; 2]], x: f64, y: f64) -> Projection {
        if let Some(index) = self
            .projection
            .iter()
            .position(|entry| entry.path == path && entry.x == x && entry.y == y)
        {
            let entry = self.projection.remove(index);
            let value = entry.value;
            self.projection.push(entry);
            return value;
        }
        let value = self.geometry(path).project(x, y);
        if self.projection.len() == 256 {
            self.projection.remove(0);
        }
        self.projection.push(ProjectionEntry {
            path: path.to_vec(),
            x,
            y,
            value,
        });
        value
    }
}

pub(super) fn project(path: &[[f64; 2]], x: f64, y: f64) -> Projection {
    CACHE.with_borrow_mut(|cache| cache.project(path, x, y))
}

pub(super) fn at(path: &[[f64; 2]], distance: f64, offset: f64) -> [f64; 2] {
    CACHE.with_borrow_mut(|cache| cache.geometry(path).at(distance, offset))
}

pub fn snapshot() -> Snapshot {
    CACHE.with_borrow(|cache| Snapshot {
        geometry: cache
            .geometry
            .iter()
            .map(|(path, value)| GeometryEntry {
                path: path.clone(),
                value: (**value).clone(),
            })
            .collect(),
        projection: cache.projection.clone(),
    })
}

pub fn restore(snapshot: Snapshot) -> Result<(), Error> {
    if snapshot.geometry.len() > 8
        || snapshot.projection.len() > 256
        || snapshot
            .geometry
            .iter()
            .any(|entry| !entry.path.is_empty() && entry.value.points.is_empty())
    {
        return Err(Error::Contract("invalid source path cache snapshot"));
    }
    CACHE.with_borrow_mut(|cache| {
        cache.geometry = snapshot
            .geometry
            .into_iter()
            .map(|entry| (entry.path, Rc::new(entry.value)))
            .collect();
        cache.projection = snapshot.projection;
    });
    Ok(())
}
