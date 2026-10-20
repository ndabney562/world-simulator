//! ADR 0028 synthetic perimeter/portal proxies, integer millimetres only.
use crate::Place;
use std::collections::BTreeSet;
use ws_rng::{Purpose, SipHasher24};
use ws_world::slice::{
    BLOCK_BUILDINGS, GeoPoint,
    population::{PopulatedSlice, WorkplaceLocation},
};

/// Projection, padding, topology and tie-breaking version.
pub const VERSION: u32 = 1;
/// Fixed walking speed per 100-ms tick, in millimetres.
pub const MM_PER_TICK: u64 = 140;
const MAX_NODES: usize = 10;

/// A point in the approximate local navigation plane, never ENU/ECEF/elevation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Point {
    /// East millimetres relative to the minimum source longitude.
    pub east: i64,
    /// North millimetres relative to the minimum source latitude.
    pub north: i64,
}
/// Invalid geometry/endpoints, disconnected graph or failed allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavError {
    /// Unsorted/duplicate points, non-positive/non-axis edges, or unsupported size/range.
    Geometry,
    /// Unknown node or semantic place.
    Endpoint,
    /// No route connects two valid endpoints.
    Disconnected,
    /// Capacity could not be reserved during construction.
    Resource,
}
impl core::fmt::Display for NavError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "navigation error: {self:?}")
    }
}
impl std::error::Error for NavError {}

/// Allocation-free shortest path in a graph with at most ten nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Path {
    nodes: [usize; MAX_NODES],
    len: usize,
    distance: u64,
}
impl Path {
    /// Full source-to-target node sequence, including a singleton for the same node.
    pub fn nodes(&self) -> &[usize] {
        &self.nodes[..self.len]
    }
    /// Integer arc length.
    pub const fn distance_mm(&self) -> u64 {
        self.distance
    }
    /// Rounded-up whole physical ticks needed to reach the endpoint.
    pub const fn walking_ticks(&self) -> u64 {
        self.distance.div_ceil(MM_PER_TICK)
    }
    fn order(&self, other: &Self) -> core::cmp::Ordering {
        self.distance
            .cmp(&other.distance)
            .then_with(|| self.nodes().cmp(other.nodes()))
    }
}
/// Small positive axis-aligned graph, with versioned lexicographic node indices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Graph {
    points: Vec<Point>,
    edges: Vec<(usize, usize)>,
}
impl Graph {
    /// Constructs from strictly sorted points (absolute coordinates <=1e9 mm)
    /// and strictly sorted unique undirected edges `(a,b)` with `a<b`.
    /// Disconnected graphs are allowed; their path queries return a typed error.
    /// # Errors
    /// Invalid size, range, ordering, endpoint or axis-aligned positive length.
    pub fn new(points: Vec<Point>, edges: Vec<(usize, usize)>) -> Result<Self, NavError> {
        if points.is_empty()
            || points.len() > MAX_NODES
            || points.windows(2).any(|w| w[0] >= w[1])
            || points.iter().any(|p| {
                p.east.unsigned_abs() > 1_000_000_000 || p.north.unsigned_abs() > 1_000_000_000
            })
            || edges.windows(2).any(|w| w[0] >= w[1])
        {
            return Err(NavError::Geometry);
        }
        for &(a, b) in &edges {
            if a >= b || b >= points.len() {
                return Err(NavError::Geometry);
            }
            let (p, q) = (points[a], points[b]);
            if (p.east == q.east) == (p.north == q.north) {
                return Err(NavError::Geometry);
            }
        }
        Ok(Self { points, edges })
    }
    /// Canonically indexed coordinates.
    pub fn points(&self) -> &[Point] {
        &self.points
    }
    /// Positive undirected edges in canonical order.
    pub fn edges(&self) -> &[(usize, usize)] {
        &self.edges
    }
    fn length(&self, a: usize, b: usize) -> u64 {
        self.points[a].east.abs_diff(self.points[b].east)
            + self.points[a].north.abs_diff(self.points[b].north)
    }
    /// Dijkstra with complete-path lexicographic tie-breaking, no heap/allocation.
    /// # Errors
    /// Unknown endpoint or disconnected endpoints.
    pub fn shortest(&self, from: usize, to: usize) -> Result<Path, NavError> {
        if from >= self.points.len() || to >= self.points.len() {
            return Err(NavError::Endpoint);
        }
        let mut best = [None::<Path>; MAX_NODES];
        let mut done = [false; MAX_NODES];
        let mut initial = Path {
            nodes: [0; MAX_NODES],
            len: 1,
            distance: 0,
        };
        initial.nodes[0] = from;
        best[from] = Some(initial);
        for _ in 0..self.points.len() {
            let Some(at) = (0..self.points.len())
                .filter(|i| !done[*i] && best[*i].is_some())
                .min_by(|a, b| best[*a].unwrap().order(&best[*b].unwrap()))
            else {
                break;
            };
            let current = best[at].unwrap();
            if at == to {
                return Ok(current);
            }
            done[at] = true;
            for &(a, b) in &self.edges {
                let next = if a == at {
                    b
                } else if b == at {
                    a
                } else {
                    continue;
                };
                if done[next] {
                    continue;
                }
                let mut path = current;
                path.nodes[path.len] = next;
                path.len += 1;
                path.distance += self.length(at, next);
                if best[next].is_none_or(|old| path.order(&old).is_lt()) {
                    best[next] = Some(path);
                }
            }
        }
        Err(NavError::Disconnected)
    }
    fn position(&self, path: &Path, mut distance: u64) -> Point {
        for edge in path.nodes().windows(2) {
            let (a, b) = (self.points[edge[0]], self.points[edge[1]]);
            let length = self.length(edge[0], edge[1]);
            if distance < length {
                // Euclidean division rounds negative displacements down. Axis-aligned
                // integer arc lengths make all sampled positions exact millimetres.
                return Point {
                    east: a.east
                        + ((i128::from(b.east - a.east) * i128::from(distance))
                            .div_euclid(i128::from(length))) as i64,
                    north: a.north
                        + ((i128::from(b.north - a.north) * i128::from(distance))
                            .div_euclid(i128::from(length))) as i64,
                };
            }
            distance -= length;
        }
        self.points[*path.nodes().last().unwrap()]
    }
}
/// Graph and semantic portal mappings; cached all-pairs routes are derived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Navigation {
    origin: GeoPoint,
    bounds: [i64; 4],
    graph: Graph,
    places: Vec<(Place, usize)>,
    paths: Vec<Path>,
}
impl Navigation {
    /// Derives perimeter/portals from the pinned extract, with no new source data.
    /// # Errors
    /// Invalid geometry/places, disconnected graph or capacity reservation failure.
    pub fn new(population: &PopulatedSlice) -> Result<Self, NavError> {
        let source: Vec<_> = BLOCK_BUILDINGS
            .iter()
            .flat_map(|b| b.footprint.polygons)
            .flat_map(|p| p.rings)
            .flat_map(|r| r.points)
            .copied()
            .collect();
        let origin = GeoPoint {
            lon_pdeg: source
                .iter()
                .map(|p| p.lon_pdeg)
                .min()
                .ok_or(NavError::Geometry)?,
            lat_pdeg: source
                .iter()
                .map(|p| p.lat_pdeg)
                .min()
                .ok_or(NavError::Geometry)?,
        };
        let project = |p: GeoPoint| -> Result<Point, NavError> {
            let east = ((i128::from(p.lon_pdeg) - i128::from(origin.lon_pdeg)) * 84_500_000)
                .div_euclid(1_000_000_000_000i128);
            let north = ((i128::from(p.lat_pdeg) - i128::from(origin.lat_pdeg)) * 111_200_000)
                .div_euclid(1_000_000_000_000i128);
            Ok(Point {
                east: i64::try_from(east).map_err(|_| NavError::Geometry)?,
                north: i64::try_from(north).map_err(|_| NavError::Geometry)?,
            })
        };
        let points = source
            .into_iter()
            .map(project)
            .collect::<Result<Vec<_>, _>>()?;
        let east = points
            .iter()
            .map(|p| p.east)
            .max()
            .ok_or(NavError::Geometry)?
            .checked_add(5000)
            .ok_or(NavError::Geometry)?;
        let north = points
            .iter()
            .map(|p| p.north)
            .max()
            .ok_or(NavError::Geometry)?
            .checked_add(5000)
            .ok_or(NavError::Geometry)?;
        let bounds = [-5000, east, -5000, north];
        let portal = |p: Point| -> Result<Point, NavError> {
            if !(-5000..=east).contains(&p.east) || !(-5000..=north).contains(&p.north) {
                return Err(NavError::Geometry);
            }
            let (_, _, point) = [
                (p.east + 5000, 0, Point { east: -5000, ..p }),
                (east - p.east, 1, Point { east, ..p }),
                (p.north + 5000, 2, Point { north: -5000, ..p }),
                (north - p.north, 3, Point { north, ..p }),
            ]
            .into_iter()
            .min_by_key(|v| (v.0, v.1))
            .unwrap();
            Ok(point)
        };
        let mut semantic = Vec::new();
        for (id, b) in population.world().buildings().iter() {
            semantic.push((Place::Building(id), portal(project(b.centroid)?)?));
        }
        for (id, w) in population.workplaces().iter() {
            if let WorkplaceLocation::Exit(e) = w.location {
                let p = *population
                    .exits()
                    .get(usize::from(e))
                    .ok_or(NavError::Endpoint)?;
                semantic.push((Place::OffMap(id, e), portal(project(p)?)?));
            }
        }
        let corners = [
            Point {
                east: -5000,
                north: -5000,
            },
            Point { east: -5000, north },
            Point { east, north: -5000 },
            Point { east, north },
        ];
        let points: Vec<_> = corners
            .into_iter()
            .chain(semantic.iter().map(|v| v.1))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut edges = BTreeSet::new();
        for (vertical, value) in [(true, -5000), (true, east), (false, -5000), (false, north)] {
            let side: Vec<_> = (0..points.len())
                .filter(|i| {
                    if vertical {
                        points[*i].east == value
                    } else {
                        points[*i].north == value
                    }
                })
                .collect();
            for pair in side.windows(2) {
                edges.insert((pair[0], pair[1]));
            }
        }
        let graph = Graph::new(points, edges.into_iter().collect())?;
        let places = semantic
            .into_iter()
            .map(|(place, p)| (place, graph.points.binary_search(&p).unwrap()))
            .collect();
        let mut paths = Vec::new();
        paths
            .try_reserve(graph.points.len() * graph.points.len())
            .map_err(|_| NavError::Resource)?;
        for a in 0..graph.points.len() {
            for b in 0..graph.points.len() {
                paths.push(graph.shortest(a, b)?);
            }
        }
        Ok(Self {
            origin,
            bounds,
            graph,
            places,
            paths,
        })
    }
    /// Approximate plane origin in the unchanged WGS84 source representation.
    pub const fn origin(&self) -> GeoPoint {
        self.origin
    }
    /// `[west,east,south,north]` of the padded proxy in millimetres.
    pub const fn bounds(&self) -> [i64; 4] {
        self.bounds
    }
    /// Immutable small graph.
    pub const fn graph(&self) -> &Graph {
        &self.graph
    }
    /// Semantic places and their graph-local node indices, never entity renumbering.
    pub fn places(&self) -> &[(Place, usize)] {
        &self.places
    }
    /// Resolves a registered building or exact workplace/exit pair.
    /// # Errors
    /// Unknown semantic endpoint.
    pub fn node(&self, place: Place) -> Result<usize, NavError> {
        self.places
            .iter()
            .find(|p| p.0 == place)
            .map(|p| p.1)
            .ok_or(NavError::Endpoint)
    }
    /// Cached immutable shortest route, no query allocation or canonical mutation.
    /// # Errors
    /// Unknown semantic endpoint.
    pub fn route(&self, from: Place, to: Place) -> Result<&Path, NavError> {
        let (a, b) = (self.node(from)?, self.node(to)?);
        Ok(&self.paths[a * self.graph.points.len() + b])
    }
    /// Exact integer path position; excess progress waits at the endpoint.
    /// # Errors
    /// Unknown semantic endpoint.
    pub fn position(&self, from: Place, to: Place, distance_mm: u64) -> Result<Point, NavError> {
        Ok(self.graph.position(self.route(from, to)?, distance_mm))
    }
    /// Explicit graph/policy/source/semantic fingerprint; route cache capacity is excluded.
    pub fn fingerprint(&self) -> u64 {
        let mut h = SipHasher24::new(0, Purpose::new("ws-life/navigation").value());
        h.write_words(&[
            u64::from(VERSION),
            ws_world::slice::DATASET_VERSION,
            self.origin.lon_pdeg as u64,
            self.origin.lat_pdeg as u64,
        ]);
        for bound in self.bounds {
            h.write_u64(bound as u64);
        }
        h.write_u64(self.graph.points.len() as u64);
        for p in &self.graph.points {
            h.write_words(&[p.east as u64, p.north as u64]);
        }
        h.write_u64(self.graph.edges.len() as u64);
        for &(a, b) in &self.graph.edges {
            h.write_words(&[a as u64, b as u64]);
        }
        h.write_u64(self.places.len() as u64);
        for &(place, node) in &self.places {
            place.visit(&mut h);
            h.write_u64(node as u64);
        }
        h.finish()
    }
}
