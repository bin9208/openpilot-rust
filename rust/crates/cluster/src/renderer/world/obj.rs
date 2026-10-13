use super::super::{Error, Result};
use num_traits::ToPrimitive;
use std::{fs, path::Path};

#[derive(Debug)]
pub struct ObjMesh {
    pub vertices: Vec<f32>,
    pub normals: Vec<f32>,
    pub colors: Vec<u8>,
}
#[derive(Clone, Copy)]
struct Face {
    vertex: usize,
    normal: Option<usize>,
}
fn index(text: &str, count: usize) -> Result<usize> {
    let raw: i128 = text
        .parse()
        .map_err(|_| Error::Contract("invalid OBJ index"))?;
    let count = i128::try_from(count).map_err(|_| Error::Contract("OBJ index count overflow"))?;
    let raw = if raw < 0 { count + raw + 1 } else { raw };
    let resolved = raw - 1;
    let resolved = if resolved < 0 {
        count + resolved
    } else {
        resolved
    };
    if resolved < 0 || resolved >= count {
        return Err(Error::Contract("OBJ index out of range"));
    }
    usize::try_from(resolved).map_err(|_| Error::Contract("OBJ index overflow"))
}
fn face(token: &str, counts: (usize, usize)) -> Result<Face> {
    let parts = token.split('/').collect::<Vec<_>>();
    let vertex = index(parts[0], counts.0)?;
    let normal = parts
        .get(2)
        .filter(|value| !value.is_empty())
        .map(|value| index(value, counts.1))
        .transpose()?;
    Ok(Face { vertex, normal })
}
fn vector(parts: &[&str]) -> Result<[f64; 3]> {
    let mut result = [0.; 3];
    for (i, value) in parts.iter().skip(1).take(3).enumerate() {
        result[i] = value
            .parse()
            .map_err(|_| Error::Contract("invalid OBJ float"))?;
    }
    Ok(result)
}
fn normal(points: [[f64; 3]; 3]) -> [f64; 3] {
    let [origin, second, third] = points;
    let edge_a = [
        second[0] - origin[0],
        second[1] - origin[1],
        second[2] - origin[2],
    ];
    let edge_b = [
        third[0] - origin[0],
        third[1] - origin[1],
        third[2] - origin[2],
    ];
    let normal = [
        edge_a[1] * edge_b[2] - edge_a[2] * edge_b[1],
        edge_a[2] * edge_b[0] - edge_a[0] * edge_b[2],
        edge_a[0] * edge_b[1] - edge_a[1] * edge_b[0],
    ];
    let length = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
    if length <= 0.000_001 {
        [0., 0., 1.]
    } else {
        [normal[0] / length, normal[1] / length, normal[2] / length]
    }
}
fn material(name: &str) -> [u8; 4] {
    match name {
        "body" => [156, 166, 172, 255],
        "besi_roda" => [36, 38, 42, 255],
        "light" => [184, 222, 255, 255],
        "stop_light" => [226, 34, 28, 255],
        "riting" => [255, 146, 20, 255],
        "Material" => [136, 142, 148, 255],
        "Material.002" => [68, 72, 78, 255],
        "wheel" | "Material.003" | "Material.004" | "Material.005" | "Material.006" => {
            [18, 20, 22, 255]
        }
        _ => [142, 150, 156, 255],
    }
}
fn utf8_ignore(mut bytes: &[u8]) -> String {
    let mut text = String::new();
    while !bytes.is_empty() {
        match std::str::from_utf8(bytes) {
            Ok(valid) => {
                text.push_str(valid);
                break;
            }
            Err(error) => {
                text.push_str(&String::from_utf8_lossy(&bytes[..error.valid_up_to()]));
                let skip = error.valid_up_to()
                    + error
                        .error_len()
                        .unwrap_or(bytes.len() - error.valid_up_to());
                bytes = &bytes[skip..];
            }
        }
    }
    text
}
impl ObjMesh {
    /// # Errors
    /// Returns file access or source OBJ parsing errors.
    pub fn load(path: &Path) -> Result<Self> {
        Self::parse(&fs::read(path)?)
    }
    /// # Errors
    /// Returns malformed geometry or invalid face-index errors.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let mut vertices = Vec::new();
        let mut normals = Vec::new();
        let mut color = material("");
        let mut result = Self {
            vertices: Vec::new(),
            normals: Vec::new(),
            colors: Vec::new(),
        };
        let text = utf8_ignore(bytes);
        for line in text.split([
            '\n', '\r', '\u{000b}', '\u{000c}', '\u{001c}', '\u{001d}', '\u{001e}', '\u{0085}',
            '\u{2028}', '\u{2029}',
        ]) {
            let parts = line
                .split(openpilot_startup_ui::text::whitespace)
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>();
            match parts.first().copied() {
                Some("v") if parts.len() >= 4 => vertices.push(vector(&parts)?),
                Some("vn") if parts.len() >= 4 => normals.push(vector(&parts)?),
                Some("usemtl") if parts.len() >= 2 => color = material(parts[1]),
                Some("f") if parts.len() >= 4 => {
                    let parsed = parts[1..]
                        .iter()
                        .map(|token| face(token, (vertices.len(), normals.len())))
                        .collect::<Result<Vec<_>>>()?;
                    for i in 1..parsed.len() - 1 {
                        let triangle = [parsed[0], parsed[i], parsed[i + 1]];
                        let fallback = normal(triangle.map(|item| vertices[item.vertex]));
                        for item in triangle {
                            for value in vertices[item.vertex] {
                                result.vertices.push(
                                    value
                                        .to_f32()
                                        .ok_or(Error::Contract("OBJ vertex overflow"))?,
                                );
                            }
                            for value in item.normal.map_or(fallback, |n| normals[n]) {
                                result.normals.push(
                                    value
                                        .to_f32()
                                        .ok_or(Error::Contract("OBJ normal overflow"))?,
                                );
                            }
                            result.colors.extend(color);
                        }
                    }
                }
                Some(_) | None => (),
            }
        }
        if result.vertices.len() < 9 || !result.vertices.len().is_multiple_of(9) {
            return Err(Error::Contract("invalid vehicle mesh vertex count"));
        }
        Ok(result)
    }
}
