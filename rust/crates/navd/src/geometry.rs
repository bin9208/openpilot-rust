use crate::Error;
use num_traits::ToPrimitive;
use serde::{Deserialize, Serialize};

const EARTH_MEAN_RADIUS: f64 = 6_371_007.2;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Coordinate {
    pub latitude: f64,
    pub longitude: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maxspeed: Option<f64>,
}

impl PartialEq for Coordinate {
    fn eq(&self, other: &Self) -> bool {
        self.latitude == other.latitude && self.longitude == other.longitude
    }
}

impl Coordinate {
    pub const fn new(latitude: f64, longitude: f64) -> Self {
        Self {
            latitude,
            longitude,
            maxspeed: None,
        }
    }

    pub fn distance_to(self, other: Self) -> Result<f64, Error> {
        let dlat = (other.latitude - self.latitude).to_radians();
        let dlon = (other.longitude - self.longitude).to_radians();
        let lat = self.latitude.to_radians();
        let other_lat = other.latitude.to_radians();
        if [dlat, dlon, lat, other_lat]
            .iter()
            .any(|value| value.is_infinite())
        {
            return Err(Error::DistanceDomain);
        }
        let mut haversine_dlat = (dlat / 2.).sin();
        haversine_dlat *= haversine_dlat;
        let mut haversine_dlon = (dlon / 2.).sin();
        haversine_dlon *= haversine_dlon;
        let y = haversine_dlat + lat.cos() * other_lat.cos() * haversine_dlon;
        if !y.is_nan() && !(0. ..=1.).contains(&y) {
            return Err(Error::DistanceDomain);
        }
        Ok(2. * y.sqrt().asin() * EARTH_MEAN_RADIUS)
    }

    fn minus(self, other: Self) -> Self {
        Self::new(
            self.latitude - other.latitude,
            self.longitude - other.longitude,
        )
    }

    fn dot(self, other: Self) -> f64 {
        self.latitude * other.latitude + self.longitude * other.longitude
    }
}

pub fn minimum_distance(a: Coordinate, b: Coordinate, position: Coordinate) -> Result<f64, Error> {
    if a.distance_to(b)? < 0.01 {
        return a.distance_to(position);
    }
    let ap = position.minus(a);
    let ab = b.minus(a);
    let t = (ap.dot(ab) / ab.dot(ab)).clamp(0., 1.);
    Coordinate::new(a.latitude + ab.latitude * t, a.longitude + ab.longitude * t)
        .distance_to(position)
}

pub fn distance_along_geometry(
    geometry: &[Coordinate],
    position: Coordinate,
) -> Result<f64, Error> {
    let first = geometry.first().ok_or(Error::EmptyGeometry)?;
    if geometry.len() <= 2 {
        return first.distance_to(position);
    }
    let mut total_distance = 0.;
    let mut total_distance_closest = 0.;
    let mut closest_distance = 1e9;
    for segment in geometry.windows(2) {
        let [a, b] = [segment[0], segment[1]];
        let distance = minimum_distance(a, b, position)?;
        if distance < closest_distance {
            closest_distance = distance;
            total_distance_closest = total_distance + a.distance_to(position)?;
        }
        total_distance += a.distance_to(b)?;
    }
    Ok(total_distance_closest)
}

pub fn limit_route_points<T: Clone>(points: &[T], maximum: usize) -> Result<Vec<T>, Error> {
    if maximum == 0 {
        return Ok(Vec::new());
    }
    if points.len() <= maximum {
        return Ok(points.to_vec());
    }
    let count = u32::try_from(points.len()).map_err(|_| Error::GeometrySize)?;
    let maximum = u32::try_from(maximum).map_err(|_| Error::GeometrySize)?;
    let mut output = Vec::with_capacity(usize::try_from(maximum).map_err(|_| Error::GeometrySize)?);
    let mut previous = None;
    for index in 0..maximum {
        let numerator = u64::from(index) * u64::from(count - 1);
        let numerator = numerator.to_f64().ok_or(Error::GeometrySize)?;
        let source = (numerator / f64::from(maximum.saturating_sub(1).max(1))).round_ties_even();
        let source = source.to_usize().ok_or(Error::GeometrySize)?;
        if previous != Some(source) {
            output.push(points.get(source).ok_or(Error::GeometrySize)?.clone());
            previous = Some(source);
        }
    }
    Ok(output)
}
