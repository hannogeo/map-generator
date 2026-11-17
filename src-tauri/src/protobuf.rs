//! Minimal protobuf wire format reader.
//!
//! Only used for Apple's GroundMetadataTile and OpenMap's Mapbox Vector Tiles.
//! Pulling in a full protobuf runtime would add megabytes to the binary for
//! two fixed schemas, so this implements the handful of wire types we need.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy)]
pub enum FieldValue<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
    Fixed64(f64),
    Fixed32(f32),
}

#[derive(Debug, Clone, Copy)]
pub struct Field<'a> {
    pub tag: u32,
    pub value: FieldValue<'a>,
}

impl<'a> Field<'a> {
    pub fn as_u64(&self) -> Option<u64> {
        match self.value {
            FieldValue::Varint(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        self.as_u64().map(|v| v as i64)
    }

    pub fn as_i32(&self) -> Option<i32> {
        self.as_u64().map(|v| v as i32)
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self.value {
            FieldValue::Fixed64(v) => Some(v),
            FieldValue::Fixed32(v) => Some(v as f64),
            FieldValue::Varint(v) => Some(v as f64),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&'a str> {
        match self.value {
            FieldValue::Bytes(b) => std::str::from_utf8(b).ok(),
            _ => None,
        }
    }

    pub fn as_bytes(&self) -> Option<&'a [u8]> {
        match self.value {
            FieldValue::Bytes(b) => Some(b),
            _ => None,
        }
    }
}

pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    pub fn is_empty(&self) -> bool {
        self.pos >= self.buf.len()
    }

    fn read_varint(&mut self) -> Option<u64> {
        let mut result: u64 = 0;
        let mut shift = 0;
        loop {
            let byte = *self.buf.get(self.pos)?;
            self.pos += 1;
            if shift < 64 {
                result |= ((byte & 0x7f) as u64) << shift;
            }
            if byte & 0x80 == 0 {
                return Some(result);
            }
            shift += 7;
            if shift > 70 {
                return None;
            }
        }
    }

    pub fn next_field(&mut self) -> Option<Field<'a>> {
        if self.is_empty() {
            return None;
        }
        let key = self.read_varint()?;
        let tag = (key >> 3) as u32;
        let wire = (key & 0x7) as u8;
        let value = match wire {
            0 => FieldValue::Varint(self.read_varint()?),
            1 => {
                let end = self.pos.checked_add(8)?;
                let bytes = self.buf.get(self.pos..end)?;
                self.pos = end;
                FieldValue::Fixed64(f64::from_le_bytes(bytes.try_into().ok()?))
            }
            2 => {
                let len = self.read_varint()? as usize;
                let end = self.pos.checked_add(len)?;
                let bytes = self.buf.get(self.pos..end)?;
                self.pos = end;
                FieldValue::Bytes(bytes)
            }
            5 => {
                let end = self.pos.checked_add(4)?;
                let bytes = self.buf.get(self.pos..end)?;
                self.pos = end;
                FieldValue::Fixed32(f32::from_le_bytes(bytes.try_into().ok()?))
            }
            _ => return None,
        };
        Some(Field { tag, value })
    }
}

pub fn packed_varints(bytes: &[u8]) -> Vec<u64> {
    let mut reader = Reader::new(bytes);
    let mut out = Vec::new();
    while !reader.is_empty() {
        match reader.read_varint() {
            Some(v) => out.push(v),
            None => break,
        }
    }
    out
}

pub fn zigzag(v: i64) -> i64 {
    if v & 1 == 1 {
        -((v + 1) / 2)
    } else {
        v / 2
    }
}

// ---------------------------------------------------------------------------
// Apple GroundMetadataTile
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct ApplePano {
    pub pano_id: u64,
    pub timestamp_ms: i64,
    pub build_idx: Option<usize>,
    pub camera_idx: Vec<usize>,
    pub x: i32,
    pub y: i32,
    pub pitch: i32,
    pub roll: i32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AppleCamera {
    pub fov_h: f64,
    pub cy: f64,
}

#[derive(Debug, Clone, Default)]
pub struct AppleTile {
    pub panos: Vec<ApplePano>,
    pub coverage_types: Vec<i32>,
    pub cameras: Vec<AppleCamera>,
}

fn parse_photo_position(bytes: &[u8]) -> ApplePano {
    let mut pano = ApplePano::default();
    let mut reader = Reader::new(bytes);
    while let Some(field) = reader.next_field() {
        match field.tag {
            1 => pano.pano_id = field.as_u64().unwrap_or(0),
            5 => pano.timestamp_ms = field.as_i64().unwrap_or(0),
            7 => pano.build_idx = field.as_u64().map(|v| v as usize),
            9 => match field.value {
                FieldValue::Varint(v) => pano.camera_idx.push(v as usize),
                FieldValue::Bytes(b) => {
                    pano.camera_idx
                        .extend(packed_varints(b).into_iter().map(|v| v as usize));
                }
                _ => {}
            },
            10 => {
                if let Some(bytes) = field.as_bytes() {
                    let mut inner = Reader::new(bytes);
                    while let Some(f) = inner.next_field() {
                        match f.tag {
                            1 => pano.x = f.as_i32().unwrap_or(0),
                            2 => pano.y = f.as_i32().unwrap_or(0),
                            5 => pano.pitch = f.as_i32().unwrap_or(0),
                            6 => pano.roll = f.as_i32().unwrap_or(0),
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }
    pano
}

fn parse_build(bytes: &[u8]) -> i32 {
    let mut reader = Reader::new(bytes);
    let mut coverage = 0;
    while let Some(field) = reader.next_field() {
        if field.tag == 6 {
            coverage = field.as_i32().unwrap_or(0);
        }
    }
    coverage
}

fn parse_camera(bytes: &[u8]) -> AppleCamera {
    let mut camera = AppleCamera::default();
    let mut reader = Reader::new(bytes);
    while let Some(field) = reader.next_field() {
        if field.tag == 4 {
            if let Some(lens) = field.as_bytes() {
                let mut inner = Reader::new(lens);
                while let Some(f) = inner.next_field() {
                    match f.tag {
                        3 => camera.fov_h = f.as_f64().unwrap_or(0.0),
                        8 => camera.cy = f.as_f64().unwrap_or(0.0),
                        _ => {}
                    }
                }
            }
        }
    }
    camera
}

pub fn parse_apple_tile(bytes: &[u8]) -> AppleTile {
    let mut tile = AppleTile::default();
    let mut reader = Reader::new(bytes);
    while let Some(field) = reader.next_field() {
        match field.tag {
            1 => {
                if let Some(bytes) = field.as_bytes() {
                    tile.panos.push(parse_photo_position(bytes));
                }
            }
            4 => {
                if let Some(bytes) = field.as_bytes() {
                    tile.coverage_types.push(parse_build(bytes));
                }
            }
            5 => {
                if let Some(bytes) = field.as_bytes() {
                    tile.cameras.push(parse_camera(bytes));
                }
            }
            _ => {}
        }
    }
    tile
}

/// Apple's camera classification used by the generation filter.
pub fn apple_camera_type(camera: &AppleCamera) -> &'static str {
    if (camera.fov_h - 1.6144296).abs() < 0.01 && (camera.cy - 0.27488935).abs() < 0.01 {
        "bigcam"
    } else if (camera.fov_h - 1.8325957).abs() < 0.01 && (camera.cy - 0.30543262).abs() < 0.01 {
        "smallcam"
    } else {
        "unknown"
    }
}

/// Apple stores heading as two skewed integers; this mirrors the reference.
pub fn apple_heading(unknown10: f64, unknown11: f64) -> f64 {
    const WESTMIN: f64 = 1.0;
    const WESTMAX: f64 = 2159.0;
    const EASTMIN: f64 = 16383.0;
    const EASTMAX: f64 = 14318.0;
    const NORTHMIN: f64 = 8204.0;
    const NORTHMAX: f64 = 6054.0;
    const SOUTHMIN: f64 = 8204.0;
    const SOUTHMAX: f64 = 10173.0;

    let ew = if unknown10 < WESTMAX {
        -((unknown10 - WESTMIN) / (WESTMAX - WESTMIN))
    } else if unknown10 > EASTMAX {
        (unknown10 - EASTMIN) / (EASTMAX - EASTMIN)
    } else {
        0.0
    };
    let ns = if unknown11 <= NORTHMIN {
        (unknown11 - NORTHMIN) / (NORTHMAX - NORTHMIN)
    } else {
        -((unknown11 - SOUTHMIN) / (SOUTHMAX - SOUTHMIN))
    };
    let mut r = ew.atan2(ns).to_degrees();
    if r < 0.0 {
        r += 360.0;
    }
    r
}

// ---------------------------------------------------------------------------
// Mapbox Vector Tiles (OpenMap)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum MvtValue {
    String(String),
    Number(f64),
    Bool(bool),
    Null,
}

impl MvtValue {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            MvtValue::Number(v) => Some(*v),
            MvtValue::Bool(v) => Some(if *v { 1.0 } else { 0.0 }),
            MvtValue::String(s) => s.parse().ok(),
            MvtValue::Null => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            MvtValue::String(s) => Some(s),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MvtFeature {
    pub id: Option<u64>,
    pub properties: HashMap<String, MvtValue>,
    pub geom_type: u32,
    pub geometry: Vec<(i64, i64)>,
}

pub fn parse_mvt(bytes: &[u8]) -> Vec<(String, i32, Vec<MvtFeature>)> {
    let mut layers = Vec::new();
    let mut reader = Reader::new(bytes);
    while let Some(field) = reader.next_field() {
        if field.tag != 3 {
            continue;
        }
        let Some(layer_bytes) = field.as_bytes() else {
            continue;
        };
        let mut name = String::new();
        let mut extent = 4096;
        let mut keys: Vec<String> = Vec::new();
        let mut values: Vec<MvtValue> = Vec::new();
        let mut raw_features: Vec<&[u8]> = Vec::new();
        let mut layer_reader = Reader::new(layer_bytes);
        while let Some(f) = layer_reader.next_field() {
            match f.tag {
                1 => name = f.as_str().unwrap_or_default().to_string(),
                5 => extent = f.as_i32().unwrap_or(4096),
                2 => {
                    if let Some(b) = f.as_bytes() {
                        raw_features.push(b);
                    }
                }
                3 => {
                    if let Some(b) = f.as_bytes() {
                        keys.push(String::from_utf8_lossy(b).into_owned());
                    }
                }
                4 => {
                    if let Some(b) = f.as_bytes() {
                        values.push(parse_mvt_value(b));
                    }
                }
                _ => {}
            }
        }

        let mut features = Vec::with_capacity(raw_features.len());
        for raw in raw_features {
            let mut feature = MvtFeature::default();
            let mut tags: Vec<u64> = Vec::new();
            let mut geometry: Vec<u64> = Vec::new();
            let mut fr = Reader::new(raw);
            while let Some(f) = fr.next_field() {
                match f.tag {
                    1 => feature.id = f.as_u64(),
                    2 => {
                        if let FieldValue::Bytes(b) = f.value {
                            tags.extend(packed_varints(b));
                        }
                    }
                    3 => feature.geom_type = f.as_u64().unwrap_or(0) as u32,
                    4 => {
                        if let FieldValue::Bytes(b) = f.value {
                            geometry.extend(packed_varints(b));
                        }
                    }
                    _ => {}
                }
            }
            for pair in tags.chunks_exact(2) {
                let key = keys.get(pair[0] as usize);
                let value = values.get(pair[1] as usize);
                if let (Some(k), Some(v)) = (key, value) {
                    feature.properties.insert(k.clone(), v.clone());
                }
            }
            feature.geometry = decode_mvt_geometry(&geometry);
            features.push(feature);
        }
        layers.push((name, extent, features));
    }
    layers
}

fn parse_mvt_value(bytes: &[u8]) -> MvtValue {
    let mut reader = Reader::new(bytes);
    while let Some(f) = reader.next_field() {
        return match f.tag {
            1 => MvtValue::String(f.as_str().unwrap_or_default().to_string()),
            2 | 3 => MvtValue::Number(f.as_f64().unwrap_or(0.0)),
            4 | 5 => MvtValue::Number(f.as_u64().unwrap_or(0) as f64),
            6 => MvtValue::Number(zigzag(f.as_u64().unwrap_or(0) as i64) as f64),
            7 => MvtValue::Bool(f.as_u64().unwrap_or(0) != 0),
            _ => MvtValue::Null,
        };
    }
    MvtValue::Null
}

fn decode_mvt_geometry(geometry: &[u64]) -> Vec<(i64, i64)> {
    let mut points = Vec::new();
    let mut i = 0;
    let (mut x, mut y) = (0i64, 0i64);
    while i < geometry.len() {
        let cmd_int = geometry[i];
        i += 1;
        let cmd = cmd_int & 0x7;
        let count = (cmd_int >> 3) as usize;
        match cmd {
            1 | 2 => {
                for _ in 0..count {
                    if i + 1 >= geometry.len() {
                        return points;
                    }
                    x += zigzag(geometry[i] as i64);
                    y += zigzag(geometry[i + 1] as i64);
                    i += 2;
                    points.push((x, y));
                }
            }
            7 => {
                if let Some(&first) = points.first() {
                    points.push(first);
                }
            }
            _ => return points,
        }
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_varints_and_bytes() {
        // field 1 varint 150, field 2 bytes "hi"
        let data = [0x08, 0x96, 0x01, 0x12, 0x02, b'h', b'i'];
        let mut reader = Reader::new(&data);
        let a = reader.next_field().unwrap();
        assert_eq!(a.tag, 1);
        assert_eq!(a.as_u64(), Some(150));
        let b = reader.next_field().unwrap();
        assert_eq!(b.tag, 2);
        assert_eq!(b.as_str(), Some("hi"));
    }

    #[test]
    fn packed_varints_roundtrip() {
        let data = [0x96, 0x01, 0x03];
        assert_eq!(packed_varints(&data), vec![150, 3]);
    }
}
