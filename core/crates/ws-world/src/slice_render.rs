//! ADR0031 immutable block meshes and explicit geodetic/render adapters.
//! Source/topology/identity are Tier1; derived continuous coordinates are Tier3.
use crate::{
    environment::site_frame,
    slice::{BLOCK_BUILDINGS, GeoPoint, SliceWorld},
};
use ws_ecs::EntityId;
use ws_math::{Enu, Geodetic, Position};

/// Fixed source/frame/triangulation and presentation-adapter version.
pub const VERSION: u32 = 1;
/// Maximum static building rows admitted by transport version1.
pub const MAX_BUILDINGS: usize = 256;
/// Maximum complete vertex count, including hard-normal wall duplicates.
pub const MAX_VERTICES: usize = 32_768;
/// Maximum complete triangle-index count.
pub const MAX_INDICES: usize = 131_072;
const PDEG: f64 = 1_000_000_000_000.0;

/// Checked construction/conversion failure, without partial publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderError {
    /// Unsupported/invalid source topology or output limits.
    Geometry,
    /// Nonfinite, out-of-range or unconvertible frame input.
    Coordinates,
    /// Temporary/output allocation could not be reserved.
    Resource,
}
impl core::fmt::Display for RenderError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "slice render error: {self:?}")
    }
}
impl std::error::Error for RenderError {}

/// Pure explicit WGS84 ellipsoidal-height conversion into the fixed block ENU.
/// # Errors
/// Invalid coordinates/height or a nonfinite result.
pub fn geo_to_render(p: GeoPoint, height_m: f64) -> Result<[f64; 3], RenderError> {
    let g = Geodetic::from_degrees(p.lat_pdeg as f64 / PDEG, p.lon_pdeg as f64 / PDEG, height_m)
        .map_err(|_| RenderError::Coordinates)?;
    let p = site_frame().to_enu(g.to_ecef());
    let out = [p.x, p.y, p.z];
    if out.iter().all(|v| v.is_finite()) {
        Ok(out)
    } else {
        Err(RenderError::Coordinates)
    }
}

/// Reverse frame conversion for a bounded coarse camera input, not navigation axes.
/// Output rounds to the existing picodegree geodetic representation.
/// # Errors
/// Nonfinite components, a component beyond1000km, or invalid geodetic conversion.
pub fn render_to_geo(p: [f64; 3]) -> Result<GeoPoint, RenderError> {
    if p.iter().any(|v| !v.is_finite() || v.abs() > 1_000_000.0) {
        return Err(RenderError::Coordinates);
    }
    let g = Geodetic::from_ecef(site_frame().to_ecef(Position::<Enu>::new(p[0], p[1], p[2])))
        .map_err(|_| RenderError::Coordinates)?;
    Ok(GeoPoint {
        lon_pdeg: (g.lon().to_degrees() * PDEG).round() as i64,
        lat_pdeg: (g.lat().to_degrees() * PDEG).round() as i64,
    })
}

/// Finite local heading, zero east and positive toward north, in[−π,π].
/// A stationary tangent has positive-zero heading.
/// # Errors
/// Nonfinite inputs or a nonfinite horizontal difference.
pub fn heading(from: [f64; 3], to: [f64; 3]) -> Result<f32, RenderError> {
    if from.iter().chain(&to).any(|v| !v.is_finite()) {
        return Err(RenderError::Coordinates);
    }
    let (east, north) = (to[0] - from[0], to[1] - from[1]);
    if !east.is_finite() || !north.is_finite() {
        return Err(RenderError::Coordinates);
    }
    if east == 0.0 && north == 0.0 {
        Ok(0.0)
    } else {
        Ok(north.atan2(east) as f32)
    }
}

/// One position and outward unit normal in native east/north/up order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshVertex {
    /// Metres in the fixed frame.
    pub position: [f32; 3],
    /// Native right-handed outward normal.
    pub normal: [f32; 3],
}
/// One source row; context BINs never become invented simulation identities.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuildingMesh {
    /// Original NYC BIN, distinct from EntityId.
    pub bin: u64,
    /// Actual featured identity, absent for source-only context.
    pub entity: Option<EntityId>,
    /// Beginning of this building's complete vertex range.
    pub first_vertex: u32,
    /// Number of vertices in that range.
    pub vertex_count: u32,
    /// Beginning of this building's triangle-index range.
    pub first_index: u32,
    /// Number of indices in that range.
    pub index_count: u32,
    /// Above-ground source height in metres, not NAVD88 elevation.
    pub height_m: f32,
    /// Source centroid east/north metres in the same render frame.
    pub centroid: [f32; 2],
}
/// Bounded immutable source geometry cached once by each native slice handle.
#[derive(Debug, Clone, PartialEq)]
pub struct Geometry {
    /// Complete source rows in ascending BIN order.
    pub buildings: Vec<BuildingMesh>,
    /// Roof/wall vertices, partitioned by building rows.
    pub vertices: Vec<MeshVertex>,
    /// Global vertex indices; native counterclockwise outward triangles.
    pub indices: Vec<u32>,
    /// Minimum east/north/up followed by maximum east/north/up.
    pub bounds: [f32; 6],
}

fn normal(a: [f64; 3], b: [f64; 3]) -> Result<[f32; 3], RenderError> {
    let n = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    let length = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if !length.is_finite() || length <= 0.0 {
        return Err(RenderError::Geometry);
    }
    Ok(n.map(|v| (v / length) as f32))
}
fn subtract(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    core::array::from_fn(|i| a[i] - b[i])
}
fn cross(a: GeoPoint, b: GeoPoint, c: GeoPoint) -> i128 {
    (i128::from(b.lon_pdeg) - i128::from(a.lon_pdeg))
        * (i128::from(c.lat_pdeg) - i128::from(a.lat_pdeg))
        - (i128::from(b.lat_pdeg) - i128::from(a.lat_pdeg))
            * (i128::from(c.lon_pdeg) - i128::from(a.lon_pdeg))
}
fn on(a: GeoPoint, b: GeoPoint, p: GeoPoint) -> bool {
    cross(a, b, p) == 0
        && (a.lon_pdeg.min(b.lon_pdeg)..=a.lon_pdeg.max(b.lon_pdeg)).contains(&p.lon_pdeg)
        && (a.lat_pdeg.min(b.lat_pdeg)..=a.lat_pdeg.max(b.lat_pdeg)).contains(&p.lat_pdeg)
}
fn intersects(a: GeoPoint, b: GeoPoint, c: GeoPoint, d: GeoPoint) -> bool {
    let (x, y, z, w) = (
        cross(a, b, c),
        cross(a, b, d),
        cross(c, d, a),
        cross(c, d, b),
    );
    (x.signum() != y.signum() && x != 0 && y != 0 && z.signum() != w.signum() && z != 0 && w != 0)
        || on(a, b, c)
        || on(a, b, d)
        || on(c, d, a)
        || on(c, d, b)
}

/// Exact topology only. Returned indices address the original nonclosing ring.
fn triangulate(ring: &[GeoPoint]) -> Result<Vec<[usize; 3]>, RenderError> {
    if ring.len() < 4 || ring.first() != ring.last() || ring.len() > MAX_VERTICES / 5 + 1 {
        return Err(RenderError::Geometry);
    }
    let p = &ring[..ring.len() - 1];
    let n = p.len();
    for (i, a) in p.iter().enumerate() {
        if a.lon_pdeg.unsigned_abs() > 180_000_000_000_000
            || a.lat_pdeg.unsigned_abs() > 90_000_000_000_000
            || p[..i].contains(a)
        {
            return Err(RenderError::Geometry);
        }
        for j in i + 1..n {
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            if intersects(*a, p[(i + 1) % n], p[j], p[(j + 1) % n]) {
                return Err(RenderError::Geometry);
            }
        }
    }
    let area: i128 = p
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let b = p[(i + 1) % n];
            i128::from(a.lon_pdeg) * i128::from(b.lat_pdeg)
                - i128::from(b.lon_pdeg) * i128::from(a.lat_pdeg)
        })
        .sum();
    if area == 0 {
        return Err(RenderError::Geometry);
    }
    let sign = area.signum();
    let mut active = Vec::new();
    active.try_reserve(n).map_err(|_| RenderError::Resource)?;
    active.extend(0..n);
    // Collinear roof vertices can be omitted; all original wall edges remain.
    loop {
        let found = (0..active.len()).find(|&i| {
            cross(
                p[active[(i + active.len() - 1) % active.len()]],
                p[active[i]],
                p[active[(i + 1) % active.len()]],
            ) == 0
        });
        match found {
            Some(i) if active.len() > 3 => {
                active.remove(i);
            }
            _ => break,
        }
    }
    let mut out = Vec::new();
    out.try_reserve(n - 2).map_err(|_| RenderError::Resource)?;
    let mut remaining = area.unsigned_abs();
    while active.len() > 3 {
        let ear = (0..active.len())
            .filter(|&i| {
                let a = active[(i + active.len() - 1) % active.len()];
                let b = active[i];
                let c = active[(i + 1) % active.len()];
                cross(p[a], p[b], p[c]) * sign > 0
                    && !active.iter().copied().any(|j| {
                        j != a
                            && j != b
                            && j != c
                            && cross(p[a], p[b], p[j]) * sign >= 0
                            && cross(p[b], p[c], p[j]) * sign >= 0
                            && cross(p[c], p[a], p[j]) * sign >= 0
                    })
            })
            // Prefer well-conditioned ears without leaving a vanishing
            // remainder. Score is integer-only; source order breaks ties.
            .max_by_key(|&i| {
                let a = p[active[(i + active.len() - 1) % active.len()]];
                let b = p[active[i]];
                let c = p[active[(i + 1) % active.len()]];
                let triangle = cross(a, b, c).unsigned_abs();
                let edge_squared = |a: GeoPoint, b: GeoPoint| {
                    let x = i128::from(a.lon_pdeg) - i128::from(b.lon_pdeg);
                    let y = i128::from(a.lat_pdeg) - i128::from(b.lat_pdeg);
                    (x * x + y * y) as u128
                };
                let perimeter_squared =
                    edge_squared(a, b) + edge_squared(b, c) + edge_squared(c, a);
                (
                    triangle.min(remaining.saturating_sub(triangle)) * 1_000_000_000
                        / perimeter_squared,
                    core::cmp::Reverse(active[i]),
                )
            })
            .ok_or(RenderError::Geometry)?;
        let mut t = [
            active[(ear + active.len() - 1) % active.len()],
            active[ear],
            active[(ear + 1) % active.len()],
        ];
        if sign < 0 {
            t.swap(0, 2);
        }
        remaining -= cross(p[t[0]], p[t[1]], p[t[2]]).unsigned_abs();
        out.push(t);
        active.remove(ear);
    }
    let mut t = [active[0], active[1], active[2]];
    if cross(p[t[0]], p[t[1]], p[t[2]]) == 0 {
        return Err(RenderError::Geometry);
    }
    if sign < 0 {
        t.swap(0, 2);
    }
    out.push(t);
    Ok(out)
}

impl Geometry {
    /// Builds all58 source extrusions with exact featured identities and no source mutation.
    /// # Errors
    /// Unsupported topology, source/identity mismatch, invalid geometry or reservation failure.
    pub fn new(world: &SliceWorld) -> Result<Self, RenderError> {
        if BLOCK_BUILDINGS.is_empty() || BLOCK_BUILDINGS.len() > MAX_BUILDINGS {
            return Err(RenderError::Geometry);
        }
        let total: usize = BLOCK_BUILDINGS
            .iter()
            .map(|b| {
                b.footprint
                    .polygons
                    .iter()
                    .flat_map(|p| p.rings)
                    .map(|r| r.points.len().saturating_sub(1))
                    .sum::<usize>()
            })
            .sum();
        if total > MAX_VERTICES / 5 || total > MAX_INDICES / 9 {
            return Err(RenderError::Geometry);
        }
        let mut result = Self {
            buildings: Vec::new(),
            vertices: Vec::new(),
            indices: Vec::new(),
            bounds: [
                f32::INFINITY,
                f32::INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
            ],
        };
        result
            .buildings
            .try_reserve(BLOCK_BUILDINGS.len())
            .map_err(|_| RenderError::Resource)?;
        result
            .vertices
            .try_reserve(total * 5)
            .map_err(|_| RenderError::Resource)?;
        result
            .indices
            .try_reserve(total * 9)
            .map_err(|_| RenderError::Resource)?;
        let mut sources: Vec<&crate::slice::SourceBuilding> = Vec::new();
        sources
            .try_reserve(BLOCK_BUILDINGS.len())
            .map_err(|_| RenderError::Resource)?;
        sources.extend(BLOCK_BUILDINGS.iter());
        sources.sort_by_key(|b| b.bin);
        for (source_index, b) in sources.iter().enumerate() {
            if b.height_mm == 0
                || b.footprint.polygons.len() != 1
                || b.footprint.polygons[0].rings.len() != 1
                || (source_index > 0 && sources[source_index - 1].bin == b.bin)
            {
                return Err(RenderError::Geometry);
            }
            let entity = world
                .buildings()
                .iter()
                .find_map(|(id, v)| (v.bin == b.bin).then_some(id));
            if b.featured.is_some() != entity.is_some() {
                return Err(RenderError::Geometry);
            }
            let ring = b.footprint.polygons[0].rings[0].points;
            let triangles = triangulate(ring)?;
            let p = &ring[..ring.len() - 1];
            let first_vertex = result.vertices.len() as u32;
            let first_index = result.indices.len() as u32;
            let height = f64::from(b.height_mm) / 1000.0;
            for &v in p {
                let pos = geo_to_render(v, height)?;
                let up = subtract(geo_to_render(v, height + 1.0)?, pos);
                let length = (up[0] * up[0] + up[1] * up[1] + up[2] * up[2]).sqrt();
                result.vertices.push(MeshVertex {
                    position: pos.map(|v| v as f32),
                    normal: up.map(|v| (v / length) as f32),
                });
            }
            for t in triangles {
                result.indices.extend(t.map(|v| first_vertex + v as u32));
            }
            let area: i128 = p
                .iter()
                .enumerate()
                .map(|(i, a)| {
                    let b = p[(i + 1) % p.len()];
                    i128::from(a.lon_pdeg) * i128::from(b.lat_pdeg)
                        - i128::from(b.lon_pdeg) * i128::from(a.lat_pdeg)
                })
                .sum();
            for i in 0..p.len() {
                let (mut a, mut b) = (p[i], p[(i + 1) % p.len()]);
                if area < 0 {
                    core::mem::swap(&mut a, &mut b);
                }
                let positions = [
                    geo_to_render(a, 0.0)?,
                    geo_to_render(b, 0.0)?,
                    geo_to_render(b, height)?,
                    geo_to_render(a, height)?,
                ];
                let n = normal(
                    subtract(positions[1], positions[0]),
                    subtract(positions[3], positions[0]),
                )?;
                let base = result.vertices.len() as u32;
                result.vertices.extend(positions.map(|position| MeshVertex {
                    position: position.map(|v| v as f32),
                    normal: n,
                }));
                result
                    .indices
                    .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            }
            let center = geo_to_render(b.centroid, 0.0)?;
            result.buildings.push(BuildingMesh {
                bin: u64::from(b.bin),
                entity,
                first_vertex,
                vertex_count: result.vertices.len() as u32 - first_vertex,
                first_index,
                index_count: result.indices.len() as u32 - first_index,
                height_m: height as f32,
                centroid: [center[0] as f32, center[1] as f32],
            });
        }
        if result
            .buildings
            .iter()
            .filter(|b| b.entity.is_some())
            .count()
            != world.buildings().len()
        {
            return Err(RenderError::Geometry);
        }
        for v in &result.vertices {
            for i in 0..3 {
                if !v.position[i].is_finite() || !v.normal[i].is_finite() {
                    return Err(RenderError::Geometry);
                }
                result.bounds[i] = result.bounds[i].min(v.position[i]);
                result.bounds[i + 3] = result.bounds[i + 3].max(v.position[i]);
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests;
