use crate::{geometry::Point, Error};
use libloading::Library;
use std::{
    ffi::{c_char, c_int, c_uint, c_void, CStr},
    path::Path,
    ptr::NonNull,
};
#[path = "geos_artifact.rs"]
mod artifact;

type Context = *mut c_void;
type Object = *mut c_void;
type Init = unsafe extern "C" fn() -> Context;
type Finish = unsafe extern "C" fn(Context);
type Create = unsafe extern "C" fn(Context, c_uint, c_uint) -> Object;
type Destroy = unsafe extern "C" fn(Context, Object);
type Set = unsafe extern "C" fn(Context, Object, c_uint, f64) -> c_int;
type Line = unsafe extern "C" fn(Context, Object) -> Object;
type Length = unsafe extern "C" fn(Context, Object, *mut f64) -> c_int;
type Interpolate = unsafe extern "C" fn(Context, Object, f64) -> Object;
type Get = unsafe extern "C" fn(Context, Object, *mut f64) -> c_int;

pub struct Geos {
    _library: Library,
    _dependency: Option<Library>,
    context: NonNull<c_void>,
    finish: Finish,
    create: Create,
    sequence_destroy: Destroy,
    set_x: Set,
    set_y: Set,
    line: Line,
    destroy: Destroy,
    length: Length,
    interpolate: Interpolate,
    get_x: Get,
    get_y: Get,
}

impl Geos {
    pub fn open(path: &Path) -> Result<Self, Error> {
        let manifest = match std::env::var_os("CARROT_GEOS_MANIFEST") {
            Some(path) => std::fs::read(path)?,
            None => include_bytes!("../native-dependencies.json").to_vec(),
        };
        let dependency_path = artifact::verify(path, &manifest)?;
        // SAFETY: manifest filename, SHA256, ELF architecture and companion hash were verified before dlopen.
        let dependency =
            Some(unsafe { Library::new(dependency_path) }.map_err(|e| Error::Geos(e.to_string()))?);
        // SAFETY: the caller selects the retained trusted GEOS C library; its C ABI is checked below.
        let library = unsafe { Library::new(path) }.map_err(|e| Error::Geos(e.to_string()))?;
        macro_rules! symbol {
            ($name:literal, $ty:ty) => {{
                // SAFETY: every symbol signature is the GEOS 3.13 C API; library remains owned by Geos.
                *unsafe { library.get::<$ty>(concat!($name, "\0").as_bytes()) }.map_err(|e| Error::Geos(e.to_string()))?
            }};
        }
        let version = symbol!("GEOSversion", unsafe extern "C" fn() -> *const c_char);
        // SAFETY: GEOSversion returns a static NUL-terminated C string in the loaded library.
        let version = unsafe { version() };
        if version.is_null() {
            return Err(Error::Geos("missing version".into()));
        }
        // SAFETY: the checked GEOSversion pointer is a static NUL-terminated C string.
        let version = unsafe { CStr::from_ptr(version) }.to_string_lossy();
        if version != "3.13.1-CAPI-1.19.2" {
            return Err(Error::Geos(format!(
                "expected 3.13.1-CAPI-1.19.2, received {version}"
            )));
        }
        let init = symbol!("GEOS_init_r", Init);
        let finish = symbol!("GEOS_finish_r", Finish);
        let create = symbol!("GEOSCoordSeq_create_r", Create);
        let sequence_destroy = symbol!("GEOSCoordSeq_destroy_r", Destroy);
        let set_x = symbol!("GEOSCoordSeq_setX_r", Set);
        let set_y = symbol!("GEOSCoordSeq_setY_r", Set);
        let line = symbol!("GEOSGeom_createLineString_r", Line);
        let destroy = symbol!("GEOSGeom_destroy_r", Destroy);
        let length = symbol!("GEOSLength_r", Length);
        let interpolate = symbol!("GEOSInterpolate_r", Interpolate);
        let get_x = symbol!("GEOSGeomGetX_r", Get);
        let get_y = symbol!("GEOSGeomGetY_r", Get);
        // SAFETY: initialization takes no arguments and returns an exclusively owned reentrant context.
        let context = NonNull::new(unsafe { init() })
            .ok_or_else(|| Error::Geos("context initialization failed".into()))?;
        Ok(Self {
            _library: library,
            _dependency: dependency,
            context,
            finish,
            create,
            sequence_destroy,
            set_x,
            set_y,
            line,
            destroy,
            length,
            interpolate,
            get_x,
            get_y,
        })
    }

    pub fn sample(&self, points: &[Point]) -> Result<(Vec<Point>, Vec<f64>), Error> {
        let count = c_uint::try_from(points.len()).map_err(|_| Error::Geometry)?;
        if count < 2 {
            return Err(Error::Geometry);
        }
        let context = self.context.as_ptr();
        // SAFETY: context is live, count matches the input, and source LineString is explicitly two-dimensional.
        let sequence =
            NonNull::new(unsafe { (self.create)(context, count, 2) }).ok_or(Error::Geometry)?;
        let mut sequence = Owned {
            api: self,
            pointer: sequence,
            kind: Kind::Sequence,
        };
        for (index, point) in points.iter().enumerate() {
            let index = c_uint::try_from(index).map_err(|_| Error::Geometry)?;
            // SAFETY: sequence belongs to this context; index is below the allocated count.
            if unsafe { (self.set_x)(context, sequence.pointer.as_ptr(), index, point.0) } != 1 {
                return Err(Error::Geometry);
            }
            // SAFETY: sequence belongs to this context; index is below the allocated count.
            if unsafe { (self.set_y)(context, sequence.pointer.as_ptr(), index, point.1) } != 1 {
                return Err(Error::Geometry);
            }
        }
        // SAFETY: GEOS takes ownership of the initialized coordinate sequence, including its error path.
        let line = unsafe { (self.line)(context, sequence.pointer.as_ptr()) };
        sequence.kind = Kind::Transferred;
        let line = Owned {
            api: self,
            pointer: NonNull::new(line).ok_or(Error::Geometry)?,
            kind: Kind::Geometry,
        };
        let mut length = 0.;
        // SAFETY: line is live in this context and output points to initialized writable f64 storage.
        if unsafe { (self.length)(context, line.pointer.as_ptr(), &mut length) } != 1 {
            return Err(Error::Geometry);
        }
        let mut output = Vec::new();
        let mut distances = Vec::new();
        let mut distance = 0.;
        while distance <= length {
            // SAFETY: line belongs to the live context; distance is an absolute, unnormalized source distance.
            let point = NonNull::new(unsafe {
                (self.interpolate)(context, line.pointer.as_ptr(), distance)
            })
            .ok_or(Error::Geometry)?;
            let point = Owned {
                api: self,
                pointer: point,
                kind: Kind::Geometry,
            };
            let (mut x, mut y) = (0., 0.);
            // SAFETY: interpolated geometry is a live GEOS point; output is initialized writable f64 storage.
            if unsafe { (self.get_x)(context, point.pointer.as_ptr(), &mut x) } != 1 {
                return Err(Error::Geometry);
            }
            // SAFETY: interpolated geometry is a live GEOS point; output is initialized writable f64 storage.
            if unsafe { (self.get_y)(context, point.pointer.as_ptr(), &mut y) } != 1 {
                return Err(Error::Geometry);
            }
            output.push((x, y));
            distances.push(distance);
            distance += 10.;
        }
        Ok((output, distances))
    }
}

enum Kind {
    Sequence,
    Geometry,
    Transferred,
}
struct Owned<'a> {
    api: &'a Geos,
    pointer: NonNull<c_void>,
    kind: Kind,
}
impl Drop for Owned<'_> {
    fn drop(&mut self) {
        let context = self.api.context.as_ptr();
        match self.kind {
            Kind::Sequence => {
                // SAFETY: untransferred sequence has one owner and was allocated in this live context.
                unsafe { (self.api.sequence_destroy)(context, self.pointer.as_ptr()) };
            }
            Kind::Geometry => {
                // SAFETY: geometry has one owner, no escaping pointers, and this context outlives it.
                unsafe { (self.api.destroy)(context, self.pointer.as_ptr()) };
            }
            Kind::Transferred => {}
        }
    }
}
impl Drop for Geos {
    fn drop(&mut self) {
        // SAFETY: borrowed geometries cannot outlive Geos; context has one owner and library remains live.
        unsafe { (self.finish)(self.context.as_ptr()) };
    }
}
