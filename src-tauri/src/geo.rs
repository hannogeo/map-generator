//! Geometry and coordinate helpers: tile math, polygon tests and the grid
//! generator used by the `grid` strategy.

use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

pub const EARTH_RADIUS_M: f64 = 6_371_000.0;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct LatLng {
    pub lat: f64,
    pub lng: f64,
}

impl LatLng {
    pub fn new(lat: f64, lng: f64) -> Self {
        Self { lat, lng }
    }
}

pub fn distance_between(a: LatLng, b: LatLng) -> f64 {
    let dlat = (b.lat - a.lat).to_radians();
    let dlng = (b.lng - a.lng).to_radians();
    let lat1 = a.lat.to_radians();
    let lat2 = b.lat.to_radians();
    let h = (dlat / 2.0).sin().powi(2)
        + (dlng / 2.0).sin().powi(2) * lat1.cos() * lat2.cos();
    2.0 * EARTH_RADIUS_M * h.sqrt().atan2((1.0 - h).max(0.0).sqrt())
}

pub fn wgs84_to_tile_coord(lat: f64, lng: f64, zoom: u32) -> (u32, u32) {
    let lat_rad = lat.to_radians();
    let scale: f64 = (1u64 << zoom) as f64;
    let x = ((lng + 180.0) / 360.0) * scale;
    let y = (1.0 - lat_rad.tan().asinh() / std::f64::consts::PI) / 2.0 * scale;
    (x.floor() as u32, y.floor() as u32)
}

/// Fractional tile coordinates (used by Apple's protobuf offsets).
pub fn wgs84_to_tile_coord_f(lat: f64, lng: f64, zoom: u32) -> (f64, f64) {
    let lat_rad = lat.to_radians();
    let scale: f64 = (1u64 << zoom) as f64;
    (
        ((lng + 180.0) / 360.0) * scale,
        (1.0 - lat_rad.tan().asinh() / std::f64::consts::PI) / 2.0 * scale,
    )
}

pub fn tile_coord_to_wgs84(x: f64, y: f64, zoom: u32) -> LatLng {
    let scale: f64 = (1u64 << zoom) as f64;
    let lon = (x / scale) * 360.0 - 180.0;
    let lat_rad = (std::f64::consts::PI * (1.0 - 2.0 * y / scale)).sinh().atan();
    LatLng::new(lat_rad.to_degrees(), lon)
}

pub fn pixel_to_lat_lng(
    x: f64,
    y: f64,
    zoom: u32,
    tile_x: u32,
    tile_y: u32,
    tile_size: f64,
) -> LatLng {
    let n = (1u64 << zoom) as f64;
    let global_x = (tile_x as f64 * tile_size + x) / tile_size;
    let global_y = (tile_y as f64 * tile_size + y) / tile_size;
    let lng = global_x / n * 360.0 - 180.0;
    let lat_rad = (std::f64::consts::PI * (1.0 - 2.0 * global_y / n)).sinh().atan();
    LatLng::new(lat_rad.to_degrees(), lng)
}

/// Global pixel coordinates at a zoom level (fractional).
pub fn lat_lng_to_pixel(lat: f64, lng: f64, zoom: u32) -> (f64, f64) {
    let scale = (1u64 << zoom) as f64;
    let x = ((lng + 180.0) / 360.0) * scale * 256.0;
    let sin_lat = lat.to_radians().sin().clamp(-0.9999, 0.9999);
    let y = (0.5 - ((1.0 + sin_lat) / (1.0 - sin_lat)).ln() / (4.0 * std::f64::consts::PI))
        * scale
        * 256.0;
    (x, y)
}

pub fn random_in_range(min: f64, max: f64) -> f64 {
    let mut rng = rand::thread_rng();
    let v = rng.gen::<f64>() * (max - min) + min;
    (v * 100.0).round() / 100.0
}

// ---------------------------------------------------------------------------
// Polygon handling
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct Bounds {
    pub south: f64,
    pub north: f64,
    pub west: f64,
    pub east: f64,
}

impl Bounds {
    pub fn center(&self) -> LatLng {
        LatLng::new(
            (self.south + self.north) / 2.0,
            (self.west + self.east) / 2.0,
        )
    }
}

/// A polygon (or multipolygon) with all of its rings. Even-odd ray casting is
/// used across every ring, which handles holes exactly like turf.js does.
#[derive(Debug, Clone)]
pub struct PolyRings {
    pub rings: Vec<Vec<[f64; 2]>>,
    pub bounds: Bounds,
}

fn parse_ring(value: &Value) -> Option<Vec<[f64; 2]>> {
    let arr = value.as_array()?;
    let mut ring = Vec::with_capacity(arr.len());
    for point in arr {
        let p = point.as_array()?;
        if p.len() < 2 {
            return None;
        }
        let lng = p[0].as_f64()?;
        let lat = p[1].as_f64()?;
        ring.push([lng, lat]);
    }
    if ring.len() < 3 {
        None
    } else {
        Some(ring)
    }
}

fn collect_rings(geometry: &Value, out: &mut Vec<Vec<[f64; 2]>>) -> Option<()> {
    let typ = geometry.get("type").and_then(|v| v.as_str());
    let empty = Value::Null;
    let coords = geometry.get("coordinates").unwrap_or(&empty);
    match typ {
        Some("Polygon") => {
            for ring in coords.as_array()? {
                if let Some(r) = parse_ring(ring) {
                    out.push(r);
                }
            }
        }
        Some("MultiPolygon") => {
            for poly in coords.as_array()? {
                for ring in poly.as_array()? {
                    if let Some(r) = parse_ring(ring) {
                        out.push(r);
                    }
                }
            }
        }
        _ => return None,
    }
    Some(())
}

impl PolyRings {
    pub fn from_geometry(value: &Value) -> Option<Self> {
        let mut rings = Vec::new();
        match value.get("type").and_then(|v| v.as_str()) {
            Some("Feature") => {
                collect_rings(value.get("geometry")?, &mut rings)?;
            }
            Some("FeatureCollection") => {
                let features = value.get("features")?.as_array()?;
                for feature in features {
                    if let Some(geom) = feature.get("geometry") {
                        collect_rings(geom, &mut rings);
                    }
                }
            }
            _ => {
                collect_rings(value, &mut rings)?;
            }
        }
        build_rings(rings)
    }

    pub fn contains(&self, lat: f64, lng: f64) -> bool {
        let mut inside = false;
        for ring in &self.rings {
            if ring_contains(ring, lng, lat) {
                inside = !inside;
            }
        }
        inside
    }

    /// Random point inside the bounding box, uniformly distributed by area.
    pub fn random_point(&self) -> LatLng {
        let mut rng = rand::thread_rng();
        let south = self.bounds.south.to_radians();
        let north = self.bounds.north.to_radians();
        let lat = (rng.gen::<f64>() * (north.sin() - south.sin()) + south.sin())
            .asin()
            .to_degrees();
        let lng = self.bounds.west + rng.gen::<f64>() * (self.bounds.east - self.bounds.west);
        LatLng::new(lat, lng)
    }
}

fn build_rings(rings: Vec<Vec<[f64; 2]>>) -> Option<PolyRings> {
    if rings.is_empty() {
        return None;
    }
    let mut south = f64::INFINITY;
    let mut north = f64::NEG_INFINITY;
    let mut west = f64::INFINITY;
    let mut east = f64::NEG_INFINITY;
    for ring in &rings {
        for p in ring {
            if p[1] < south {
                south = p[1];
            }
            if p[1] > north {
                north = p[1];
            }
            if p[0] < west {
                west = p[0];
            }
            if p[0] > east {
                east = p[0];
            }
        }
    }
    if !south.is_finite() || !north.is_finite() || !west.is_finite() || !east.is_finite() {
        return None;
    }
    Some(PolyRings {
        rings,
        bounds: Bounds {
            south,
            north,
            west,
            east,
        },
    })
}

pub fn ring_contains(ring: &[[f64; 2]], x: f64, y: f64) -> bool {
    let n = ring.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = (ring[i][0], ring[i][1]);
        let (xj, yj) = (ring[j][0], ring[j][1]);
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Point in polygon without holes, used for GeoJSON feature tests.
pub fn point_in_rings(rings: &[Vec<[f64; 2]>], lat: f64, lng: f64) -> bool {
    let mut inside = false;
    for ring in rings {
        if ring_contains(ring, lng, lat) {
            inside = !inside;
        }
    }
    inside
}

/// Parses a GeoJSON Feature/FeatureCollection into `(rings, properties)`.
pub fn parse_geojson(value: &Value) -> Option<(PolyRings, Vec<Value>)> {
    let props = match value.get("type").and_then(|v| v.as_str()) {
        Some("Feature") => vec![value.get("properties").cloned().unwrap_or(Value::Null)],
        Some("FeatureCollection") => value
            .get("features")
            .and_then(|f| f.as_array())
            .map(|features| {
                features
                    .iter()
                    .map(|f| f.get("properties").cloned().unwrap_or(Value::Null))
                    .collect()
            })
            .unwrap_or_default(),
        _ => vec![Value::Null],
    };
    Some((PolyRings::from_geometry(value)?, props))
}

/// Visvalingam-Whyatt simplification used by the GeoJSON downloader.
pub fn simplify_ring(points: &[[f64; 2]], target_count: usize) -> Vec<[f64; 2]> {
    if points.len() <= target_count || target_count < 3 {
        return points.to_vec();
    }
    let mut areas: Vec<(usize, f64)> = Vec::with_capacity(points.len());
    for i in 0..points.len() {
        let area = if i == 0 || i == points.len() - 1 {
            f64::INFINITY
        } else {
            let a = points[i - 1];
            let b = points[i];
            let c = points[i + 1];
            ((a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1])) / 2.0)
                .abs()
        };
        areas.push((i, area));
    }
    areas.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let keep: HashSet<usize> = areas
        .iter()
        .take(target_count)
        .map(|(i, _)| *i)
        .collect();
    points
        .iter()
        .enumerate()
        .filter(|(i, _)| keep.contains(i))
        .map(|(_, p)| *p)
        .collect()
}

// ---------------------------------------------------------------------------
// Accepted curve
// ---------------------------------------------------------------------------

pub fn accepts_curve(headings: &[f64], min_curve_angle: f64) -> bool {
    if headings.len() != 2 {
        return false;
    }
    let diff = (headings[0] - headings[1]).abs() % 360.0;
    let smallest = if diff > 180.0 { 360.0 - diff } else { diff };
    (180.0 - smallest).abs() >= min_curve_angle
}

/// Converts Mapy.cz's omega/phi/kappa angles (degrees) to heading/pitch/roll
/// in degrees.
pub fn opk_to_hpr(omega: f64, phi: f64, kappa: f64) -> (f64, f64, f64) {
    let o = omega.to_radians();
    let p = phi.to_radians();
    let k = kappa.to_radians();
    let (cz, sz) = (p.cos(), p.sin());
    let (cx, sx) = ((-o).cos(), (-o).sin());
    let (cy, sy) = (k.cos(), k.sin());
    let m = [
        [
            cz * cy - sz * sx * sy,
            -sz * cx,
            cz * sy + sz * sx * cy,
        ],
        [
            sz * cy + cz * sx * sy,
            cz * cx,
            sz * sy - cz * sx * cy,
        ],
        [-cx * sy, sx, cx * cy],
    ];
    let (heading, pitch, roll) = if m[2][1].abs() < 0.999999 {
        (
            (-m[2][0]).atan2(m[2][2]),
            m[2][1].asin(),
            (-m[0][1]).atan2(m[1][1]),
        )
    } else {
        (
            m[1][0].atan2(m[0][0]),
            std::f64::consts::FRAC_PI_2 * m[2][1].signum(),
            0.0,
        )
    };
    (heading.to_degrees(), pitch.to_degrees(), roll.to_degrees())
}

// ---------------------------------------------------------------------------
// Grid generator
// ---------------------------------------------------------------------------

const GOLDEN: f64 = 0.618033988749895;
const BITSET_LIMIT: usize = 64 * 1024 * 1024 * 8; // 64 MiB of bits

#[derive(Debug, Clone, Copy)]
struct Edge {
    y_min: f64,
    y_max: f64,
    x_at_ymin: f64,
    inv_slope: f64,
}

/// Streaming grid point generator. Mirrors the reference implementation:
/// rows spaced `radius * 2 * 0.866`, alternating half-step row offsets and a
/// different pseudo random offset per pass. Points that were handed out in a
/// previous pass are skipped via a compact bitset.
pub struct GridGenerator {
    bounds: Bounds,
    lat_step: f64,
    lng_step: f64,
    rows: i64,
    cols: i64,
    edges: Vec<Edge>,
    sorted: Vec<usize>,
    active: Vec<usize>,
    sorted_ptr: usize,

    iteration: u64,
    row: i64,
    row_ready: bool,
    pairs: Vec<(f64, f64)>,
    pair_i: usize,
    col: i64,
    col_end: i64,
    total_lng_offset: f64,
    lat: f64,
    lat_idx: i64,
    emitted_this_pass: u64,
    visited: Vec<u64>,
    bits: usize,
}

impl GridGenerator {
    pub fn new(rings: &PolyRings, radius_meters: f64) -> Result<Self, String> {
        let radius = if radius_meters > 0.0 {
            radius_meters
        } else {
            50.0
        };
        let eps = 1e-12;
        let bounds = Bounds {
            south: rings.bounds.south - eps,
            north: rings.bounds.north + eps,
            west: rings.bounds.west - eps,
            east: rings.bounds.east + eps,
        };
        let spacing = radius * 2.0 * 0.866;
        let mid_lat = (bounds.south + bounds.north) / 2.0;
        let meters_per_deg_lat = 111_320.0;
        let meters_per_deg_lng =
            111_320.0 * (mid_lat * std::f64::consts::PI / 180.0).cos().max(1e-6);
        let lat_step = spacing / meters_per_deg_lat;
        let lng_step = spacing / meters_per_deg_lng;
        let rows = ((bounds.north - bounds.south) / lat_step).floor() as i64 + 1;
        let cols = ((bounds.east - bounds.west) / lng_step).floor() as i64 + 1;

        if rows <= 0 || cols <= 0 {
            return Err("Polygon is too small for this radius".into());
        }
        let bits = (rows as u128 + 1) * cols as u128;
        if bits > BITSET_LIMIT as u128 {
            return Err(format!(
                "Grid is too large ({} x {} cells). Use a larger radius or a smaller area.",
                rows, cols
            ));
        }

        let mut edges = Vec::new();
        for ring in &rings.rings {
            let n = ring.len();
            for i in 0..n {
                let (x1, y1) = (ring[i][0], ring[i][1]);
                let (x2, y2) = (ring[(i + 1) % n][0], ring[(i + 1) % n][1]);
                if (y2 - y1).abs() < 1e-12 {
                    continue;
                }
                if y1 < y2 {
                    edges.push(Edge {
                        y_min: y1,
                        y_max: y2,
                        x_at_ymin: x1,
                        inv_slope: (x2 - x1) / (y2 - y1),
                    });
                } else {
                    edges.push(Edge {
                        y_min: y2,
                        y_max: y1,
                        x_at_ymin: x2,
                        inv_slope: (x1 - x2) / (y1 - y2),
                    });
                }
            }
        }
        let mut sorted: Vec<usize> = (0..edges.len()).collect();
        sorted.sort_by(|a, b| {
            edges[*a]
                .y_min
                .partial_cmp(&edges[*b].y_min)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let words = ((bits as usize) + 63) / 64;
        Ok(Self {
            bounds,
            lat_step,
            lng_step,
            rows,
            cols,
            edges,
            sorted,
            active: Vec::new(),
            sorted_ptr: 0,
            iteration: 0,
            row: 0,
            row_ready: false,
            pairs: Vec::new(),
            pair_i: 0,
            col: 0,
            col_end: -1,
            total_lng_offset: 0.0,
            lat: 0.0,
            lat_idx: 0,
            emitted_this_pass: 0,
            visited: vec![0u64; words],
            bits: bits as usize,
        })
    }

    pub fn rows(&self) -> i64 {
        self.rows
    }

    pub fn cols(&self) -> i64 {
        self.cols
    }

    fn test_bit(&self, index: usize) -> bool {
        self.visited[index >> 6] & (1u64 << (index & 63)) != 0
    }

    fn set_bit(&mut self, index: usize) {
        self.visited[index >> 6] |= 1u64 << (index & 63);
    }

    fn prepare_row(&mut self) -> bool {
        let frac_lat = ((self.iteration as f64) * GOLDEN) % 1.0;
        let frac_lng = ((self.iteration as f64) * 0.7548776662466927) % 1.0;
        let lat_offset = frac_lat * self.lat_step;
        let lng_offset = frac_lng * self.lng_step;
        let lat = self.bounds.south + lat_offset + self.row as f64 * self.lat_step;
        if lat > self.bounds.north + self.lat_step {
            return false;
        }
        self.lat = lat;

        while self.sorted_ptr < self.sorted.len() {
            let ei = self.sorted[self.sorted_ptr];
            let edge = self.edges[ei];
            if edge.y_min > lat {
                break;
            }
            if lat < edge.y_max {
                self.active.push(ei);
            }
            self.sorted_ptr += 1;
        }
        self.active.retain(|&ei| lat < self.edges[ei].y_max);
        if self.active.is_empty() {
            return false;
        }

        let mut xs: Vec<f64> = self
            .active
            .iter()
            .map(|&ei| {
                let e = self.edges[ei];
                e.x_at_ymin + (lat - e.y_min) * e.inv_slope
            })
            .collect();
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        xs.dedup_by(|a, b| (*a - *b).abs() <= 1e-9);
        if xs.len() % 2 == 1 {
            xs.pop();
        }
        if xs.is_empty() {
            return false;
        }
        self.pairs = xs.chunks_exact(2).map(|c| (c[0], c[1])).collect();
        self.pair_i = 0;
        self.col = i64::MIN;
        self.total_lng_offset = lng_offset
            + if self.row & 1 == 1 {
                self.lng_step * 0.5
            } else {
                0.0
            };
        self.lat_idx = ((lat - self.bounds.south) / self.lat_step).round() as i64;
        true
    }

    /// Returns the next unvisited grid point, or `None` after a complete pass
    /// produced no new points.
    pub fn next_point(&mut self) -> Option<LatLng> {
        loop {
            if !self.row_ready {
                if self.row >= self.rows {
                    if self.emitted_this_pass == 0 {
                        // Start a fresh pass with a different offset but let the
                        // caller breathe so we do not spin on fully covered areas.
                        self.iteration += 1;
                        self.row = 0;
                        self.sorted_ptr = 0;
                        self.active.clear();
                        self.row_ready = false;
                        self.emitted_this_pass = 0;
                        return None;
                    }
                    self.iteration += 1;
                    self.row = 0;
                    self.sorted_ptr = 0;
                    self.active.clear();
                    self.emitted_this_pass = 0;
                    self.row_ready = false;
                    continue;
                }
                self.row_ready = self.prepare_row();
                if !self.row_ready {
                    self.row += 1;
                    continue;
                }
            }

            while self.pair_i < self.pairs.len() {
                let (x_l, x_r) = self.pairs[self.pair_i];
                if x_r <= self.bounds.west || x_l >= self.bounds.east {
                    self.pair_i += 1;
                    self.col = i64::MIN;
                    continue;
                }
                let min_lng = x_l.max(self.bounds.west);
                let max_lng = x_r.min(self.bounds.east);
                if self.col == i64::MIN {
                    self.col = ((min_lng - self.bounds.west - self.total_lng_offset)
                        / self.lng_step)
                        .ceil() as i64;
                    self.col_end = ((max_lng - self.bounds.west - self.total_lng_offset)
                        / self.lng_step)
                        .floor() as i64;
                }
                while self.col <= self.col_end {
                    let col = self.col;
                    self.col += 1;
                    let bit_index = self.lat_idx * self.cols + col;
                    if bit_index < 0 || bit_index as usize >= self.bits {
                        continue;
                    }
                    let idx = bit_index as usize;
                    if self.test_bit(idx) {
                        continue;
                    }
                    self.set_bit(idx);
                    let lng = self.bounds.west
                        + col as f64 * self.lng_step
                        + self.total_lng_offset;
                    self.emitted_this_pass += 1;
                    return Some(LatLng::new(self.lat, lng));
                }
                self.pair_i += 1;
                self.col = i64::MIN;
            }
            self.row_ready = false;
            self.row += 1;
        }
    }

    /// Number of points handed out in the current pass.
    pub fn emitted_this_pass(&self) -> u64 {
        self.emitted_this_pass
    }
}

/// Grid point jitter state persisted across pause/resume lives in the
/// generator itself, so a fresh engine only needs to keep the generator around
/// for the lifetime of a run.

// ---------------------------------------------------------------------------
// Baidu coordinate conversion (WGS84 -> BD09MC and BD09MC -> GCJ02)
// ---------------------------------------------------------------------------

const X_PI: f64 = std::f64::consts::PI * 3000.0 / 180.0;
const GCJ_A: f64 = 6_378_245.0;
const GCJ_EE: f64 = 0.00669342162296594323;

const MCBAND: [f64; 6] = [
    12890594.86,
    8362377.87,
    5591021.0,
    3481989.83,
    1678043.12,
    0.0,
];
const LLBAND: [f64; 6] = [75.0, 60.0, 45.0, 30.0, 15.0, 0.0];

const MC2LL: [[f64; 10]; 6] = [
    [
        1.410526172116255e-8,
        0.00000898305509648872,
        -1.9939833816331,
        200.9824383106796,
        -187.2403703815547,
        91.6087516669843,
        -23.38765649603339,
        2.57121317296198,
        -0.03801003308653,
        17337981.2,
    ],
    [
        -7.435856389565537e-9,
        0.000008983055097726239,
        -0.78625201886289,
        96.32687599759846,
        -1.85204757529826,
        -59.36935905485877,
        47.40033549296737,
        -16.50741931063887,
        2.28786674699375,
        10260144.86,
    ],
    [
        -3.030883460898826e-8,
        0.00000898305509983578,
        0.30071316287616,
        59.74293618442277,
        7.357984074871,
        -25.38371002664745,
        13.45380521110908,
        -3.29883767235584,
        0.32710905363475,
        6856817.37,
    ],
    [
        -1.981981304930552e-8,
        0.000008983055099779535,
        0.03278182852591,
        40.31678527705744,
        0.65659298677277,
        -4.44255534477492,
        0.85341911805263,
        0.12923347998204,
        -0.04625736007561,
        4482777.06,
    ],
    [
        3.09191371068437e-9,
        0.000008983055096812155,
        0.00006995724062,
        23.10934304144901,
        -0.00023663490511,
        -0.6321817810242,
        -0.00663494467273,
        0.03430082397953,
        -0.00466043876332,
        2555164.4,
    ],
    [
        2.890871144776878e-9,
        0.000008983055095805407,
        -3.068298e-8,
        7.47137025468032,
        -0.00000353937994,
        -0.02145144861037,
        -0.00001234426596,
        0.00010322952773,
        -0.00000323890364,
        826088.5,
    ],
];

const LL2MC: [[f64; 10]; 6] = [
    [
        -0.0015702102444,
        111320.7020616939,
        1704480524535203.0,
        -10338987376042340.0,
        26112667856603880.0,
        -35149669176653700.0,
        26595700718403920.0,
        -10725012454188240.0,
        1800819912950474.0,
        82.5,
    ],
    [
        0.0008277824516172526,
        111320.7020463578,
        647795574.6671607,
        -4082003173.641316,
        10774905663.51142,
        -15171875531.51559,
        12053065338.62167,
        -5124939663.577472,
        913311935.9512032,
        67.5,
    ],
    [
        0.00337398766765,
        111320.7020202162,
        4481351.045890365,
        -23393751.19931662,
        79682215.47186455,
        -115964993.2797253,
        97236711.15602145,
        -43661946.33752821,
        8477230.501135234,
        52.5,
    ],
    [
        0.00220636496208,
        111320.7020209128,
        51751.86112841131,
        3796837.749470245,
        992013.7397791013,
        -1221952.21711287,
        1340652.697009075,
        -620943.6990984312,
        144416.9293806241,
        37.5,
    ],
    [
        -0.0003441963504368392,
        111320.7020576856,
        278.2353980772752,
        2485758.690035394,
        6070.750963243378,
        54821.18345352118,
        9540.606633304236,
        -2710.55326746645,
        1405.483844121726,
        22.5,
    ],
    [
        -0.0003218135878613132,
        111320.7020701615,
        0.00369383431289,
        823725.6402795718,
        0.46104986909093,
        2351.343141331292,
        1.58060784298199,
        8.77738589078284,
        0.37238884252424,
        7.45,
    ],
];

fn transform_lat(x: f64, y: f64) -> f64 {
    let mut ret = -100.0
        + 2.0 * x
        + 3.0 * y
        + 0.2 * y * y
        + 0.1 * x * y
        + 0.2 * x.abs().sqrt();
    ret += (20.0 * (6.0 * x * std::f64::consts::PI).sin()
        + 20.0 * (2.0 * x * std::f64::consts::PI).sin())
        * 2.0
        / 3.0;
    ret += (20.0 * (y * std::f64::consts::PI).sin()
        + 40.0 * (y / 3.0 * std::f64::consts::PI).sin())
        * 2.0
        / 3.0;
    ret += (160.0 * (y / 12.0 * std::f64::consts::PI).sin()
        + 320.0 * (y * std::f64::consts::PI / 30.0).sin())
        * 2.0
        / 3.0;
    ret
}

fn transform_lng(x: f64, y: f64) -> f64 {
    let mut ret = 300.0
        + x
        + 2.0 * y
        + 0.1 * x * x
        + 0.1 * x * y
        + 0.1 * x.abs().sqrt();
    ret += (20.0 * (6.0 * x * std::f64::consts::PI).sin()
        + 20.0 * (2.0 * x * std::f64::consts::PI).sin())
        * 2.0
        / 3.0;
    ret += (20.0 * (x * std::f64::consts::PI).sin()
        + 40.0 * (x / 3.0 * std::f64::consts::PI).sin())
        * 2.0
        / 3.0;
    ret += (150.0 * (x / 12.0 * std::f64::consts::PI).sin()
        + 300.0 * (x / 30.0 * std::f64::consts::PI).sin())
        * 2.0
        / 3.0;
    ret
}

fn out_of_china(lng: f64, lat: f64) -> bool {
    !(72.004..=137.8347).contains(&lng) || !(0.8293..=55.8271).contains(&lat)
}

/// WGS84 -> GCJ02 (China offset).
pub fn wgs84_to_gcj02(lng: f64, lat: f64) -> (f64, f64) {
    if out_of_china(lng, lat) {
        return (lng, lat);
    }
    let mut d_lat = transform_lat(lng - 105.0, lat - 35.0);
    let mut d_lng = transform_lng(lng - 105.0, lat - 35.0);
    let rad_lat = lat / 180.0 * std::f64::consts::PI;
    let mut magic = rad_lat.sin();
    magic = 1.0 - GCJ_EE * magic * magic;
    let sqrt_magic = magic.sqrt();
    d_lat = (d_lat * 180.0)
        / ((GCJ_A * (1.0 - GCJ_EE)) / (magic * sqrt_magic) * std::f64::consts::PI);
    d_lng = (d_lng * 180.0)
        / (GCJ_A / sqrt_magic * rad_lat.cos() * std::f64::consts::PI);
    (lng + d_lng, lat + d_lat)
}

pub fn gcj02_to_bd09(lng: f64, lat: f64) -> (f64, f64) {
    let z = (lng * lng + lat * lat).sqrt() + 0.00002 * (lat * X_PI).sin();
    let theta = lat.atan2(lng) + 0.000003 * (lng * X_PI).cos();
    (z * theta.cos() + 0.0065, z * theta.sin() + 0.006)
}

pub fn bd09_to_gcj02(lng: f64, lat: f64) -> (f64, f64) {
    let x = lng - 0.0065;
    let y = lat - 0.006;
    let z = (x * x + y * y).sqrt() - 0.00002 * (y * X_PI).sin();
    let theta = y.atan2(x) - 0.000003 * (x * X_PI).cos();
    (z * theta.cos(), z * theta.sin())
}

fn convertor(x: f64, y: f64, param: &[f64; 10]) -> (f64, f64) {
    let x_temp = param[0] + param[1] * x.abs();
    let c_d = y.abs() / param[9];
    let y_temp = param[2]
        + param[3] * c_d
        + param[4] * c_d.powi(2)
        + param[5] * c_d.powi(3)
        + param[6] * c_d.powi(4)
        + param[7] * c_d.powi(5)
        + param[8] * c_d.powi(6);
    (
        x_temp * if x < 0.0 { -1.0 } else { 1.0 },
        y_temp * if y < 0.0 { -1.0 } else { 1.0 },
    )
}

/// WGS84 -> BD09MC (the projection used by Baidu's street view API).
pub fn wgs84_to_bd09mc(lng: f64, lat: f64) -> (f64, f64) {
    let (g_lng, g_lat) = wgs84_to_gcj02(lng, lat);
    let (b_lng, b_lat) = gcj02_to_bd09(g_lng, g_lat);
    let lng2 = ((b_lng + 180.0).rem_euclid(360.0)) - 180.0;
    let lat2 = b_lat.clamp(-74.0, 74.0);
    let mut params = &LL2MC[LL2MC.len() - 1];
    for i in 0..LLBAND.len() {
        if lat2 >= LLBAND[i] {
            params = &LL2MC[i];
            break;
        }
    }
    convertor(lng2, lat2, params)
}

/// BD09MC -> GCJ02, matching the reference `gcoord` conversion.
pub fn bd09mc_to_gcj02(x: f64, y: f64) -> (f64, f64) {
    let abs_lat = y.abs();
    for i in 0..MCBAND.len() {
        if abs_lat >= MCBAND[i] {
            let (lng, lat) = convertor(x, y, &MC2LL[i]);
            return bd09_to_gcj02(lng, lat);
        }
    }
    bd09_to_gcj02(x, y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn polygon_contains_with_hole() {
        let geom = json!({
            "type": "Polygon",
            "coordinates": [
                [[0.0,0.0],[10.0,0.0],[10.0,10.0],[0.0,10.0],[0.0,0.0]],
                [[4.0,4.0],[6.0,4.0],[6.0,6.0],[4.0,6.0],[4.0,4.0]]
            ]
        });
        let poly = PolyRings::from_geometry(&geom).unwrap();
        assert!(poly.contains(1.0, 1.0));
        assert!(!poly.contains(5.0, 5.0));
        assert!(!poly.contains(20.0, 5.0));
    }

    #[test]
    fn tile_roundtrip() {
        let (x, y) = wgs84_to_tile_coord_f(48.8584, 2.2945, 17);
        let ll = tile_coord_to_wgs84(x, y, 17);
        assert!((ll.lat - 48.8584).abs() < 1e-6);
        assert!((ll.lng - 2.2945).abs() < 1e-6);
    }

    #[test]
    fn bd09mc_roundtrip_reasonable() {
        let (x, y) = wgs84_to_bd09mc(116.3975, 39.9087);
        assert!(x > 12_900_000.0 && x < 13_100_000.0);
        let (lng, lat) = bd09mc_to_gcj02(x, y);
        assert!((lng - 116.3975).abs() < 0.02);
        assert!((lat - 39.9087).abs() < 0.02);
    }

    #[test]
    fn grid_generates_unique_points() {
        let geom = json!({
            "type": "Polygon",
            "coordinates": [[[0.0,0.0],[0.01,0.0],[0.01,0.01],[0.0,0.01],[0.0,0.0]]]
        });
        let poly = PolyRings::from_geometry(&geom).unwrap();
        let mut grid = GridGenerator::new(&poly, 100.0).unwrap();
        let mut seen = HashSet::new();
        for _ in 0..50 {
            if let Some(p) = grid.next_point() {
                let key = ((p.lat * 1e9) as i64, (p.lng * 1e9) as i64);
                assert!(seen.insert(key), "duplicate grid point");
                assert!(poly.contains(p.lat, p.lng));
            }
        }
    }
}
