//! VS-002 structural baseline for the four ADR 0023 buildings.
//!
//! Source coordinates and dimensions are exact integers (Tier 1). Conversion
//! to ADR 0015 surface positions uses `ws-math`, never identity arithmetic.
//! This is baseline data, with no residents, mutable deltas, or render state.
//!
//! ```
//! use ws_world::slice::SliceWorld;
//! let world = SliceWorld::new(42)?;
//! assert_eq!(world.buildings().len(), 4);
//! for (id, building) in world.buildings().iter() {
//!     // The pipeline supplies ellipsoidal terrain height later. Zero here
//!     // explicitly means the WGS84 ellipsoid, not surveyed street level.
//!     let position = building.centroid.surface_position(0)?;
//!     assert_eq!(position.cell(), position.occupied_cell()?);
//!     assert!(building.height_mm > 0);
//!     let _ = id;
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use crate::ids;
use core::fmt;
use std::collections::BTreeSet;
use ws_ecs::{Column, EntityId};
use ws_math::{CoordError, Geodetic, SurfacePos};

#[path = "block_6049.rs"]
mod block;
pub use block::{BLOCK_BUILDINGS, DATASET_VERSION, EXTRACT_SHA256};

/// Version of the structural loader; changes to its output require review.
pub const GENERATOR_VERSION: u32 = 1;
const PDEG: i64 = 1_000_000_000_000;

/// Exact WGS84 longitude/latitude in picodegrees (all 12 source decimals).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeoPoint {
    /// Longitude, in 10^-12 degrees east.
    pub lon_pdeg: i64,
    /// Latitude, in 10^-12 degrees north.
    pub lat_pdeg: i64,
}

impl GeoPoint {
    /// Converts at an explicitly supplied WGS84 ellipsoidal ground height.
    /// The extract's NAVD88 ground height must NOT be passed as ellipsoidal
    /// height without a pipeline datum conversion (ADR 0015).
    ///
    /// # Errors
    /// Invalid latitude/longitude, or an unresolved surface-cell boundary.
    pub fn surface_position(self, ellipsoidal_height_mm: i64) -> Result<SurfacePos, CoordError> {
        let geodetic = Geodetic::from_degrees(
            self.lat_pdeg as f64 / PDEG as f64,
            self.lon_pdeg as f64 / PDEG as f64,
            ellipsoidal_height_mm as f64 / 1000.0,
        )?;
        SurfacePos::from_ecef(geodetic.to_ecef())
    }

    fn valid(self) -> bool {
        (-180 * PDEG..=180 * PDEG).contains(&self.lon_pdeg)
            && (-90 * PDEG..=90 * PDEG).contains(&self.lat_pdeg)
    }
}

/// A closed ring, with its closing vertex retained verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ring {
    /// Source vertex order; at least three distinct vertices plus closure.
    pub points: &'static [GeoPoint],
}

/// One polygon: the exterior ring followed by any interior rings (holes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Polygon {
    /// Rings are never flattened, reordered, or simplified.
    pub rings: &'static [Ring],
}

/// A MultiPolygon footprint. Parts, holes, winding, and closure are retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Footprint {
    /// Polygons in source order.
    pub polygons: &'static [Polygon],
}

impl Footprint {
    /// Checks shape, coordinates and nonzero ring area using only integers.
    /// This validates representation, not general polygon topology.
    pub fn is_valid(self) -> bool {
        !self.polygons.is_empty()
            && self.polygons.iter().all(|polygon| {
                !polygon.rings.is_empty()
                    && polygon.rings.iter().all(|ring| {
                        let p = ring.points;
                        p.len() >= 4
                            && p.first() == p.last()
                            && p.iter().all(|v| v.valid())
                            && p.windows(2)
                                .map(|w| {
                                    i128::from(w[0].lon_pdeg) * i128::from(w[1].lat_pdeg)
                                        - i128::from(w[1].lon_pdeg) * i128::from(w[0].lat_pdeg)
                                })
                                .sum::<i128>()
                                != 0
                    })
            })
    }
}

/// One reviewed extract row, including the address-level generation key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceBuilding {
    /// NYC Building Identification Number; generation address, NOT EntityId.
    pub bin: u32,
    /// Borough-block-lot identifier, retained independently from the BIN.
    pub bbl: u64,
    /// Tax lot number.
    pub lot: u32,
    /// PLUTO address (no real-person data).
    pub address: &'static str,
    /// PLUTO building/use class, preserved without inferred reclassification.
    pub building_class: &'static str,
    /// Source feature code (2100 building, 5110 garage).
    pub feature_code: u32,
    /// Detailed-simulation role, or None for surrounding context.
    pub featured: Option<&'static str>,
    /// Roof height ABOVE GROUND, in millimetres.
    pub height_mm: u32,
    /// Original ground elevation in NAVD88 millimetres; not ellipsoidal.
    pub ground_navd88_mm: i32,
    /// PLUTO floors in thousandths; per lot, including on garage rows.
    pub floors_milli: u32,
    /// PLUTO residential units; per lot, not per footprint.
    pub residential_units: u32,
    /// Extract centroid, at its stated six-decimal precision.
    pub centroid: GeoPoint,
    /// Full source geometry at twelve-decimal precision.
    pub footprint: Footprint,
}

/// Explicit inputs for regenerating the structural baseline (future save header).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Baseline {
    /// World seed.
    pub seed: u64,
    /// Version of the reviewed extract.
    pub dataset_version: u64,
    /// Structural loader version.
    pub generator_version: u32,
}

/// Invalid source data or a detected procedural ID collision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliceError {
    /// Duplicate source key, before hashing.
    DuplicateBin(u32),
    /// Malformed footprint, centroid, or height for this BIN.
    InvalidBuilding(u32),
    /// Distinct generation addresses hashed to the same EntityId (#30).
    Collision(EntityId),
}

impl fmt::Display for SliceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateBin(bin) => write!(f, "duplicate source BIN {bin}"),
            Self::InvalidBuilding(bin) => write!(f, "invalid source building {bin}"),
            Self::Collision(id) => write!(f, "procedural building ID collision: {id}"),
        }
    }
}
impl std::error::Error for SliceError {}

/// Authoritative structural baseline of the four featured buildings.
/// Source context stays accessible separately; no residents are generated.
/// All entity iteration follows ascending EntityId order (ADR 0016).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SliceWorld {
    baseline: Baseline,
    buildings: Column<SourceBuilding>,
}

impl SliceWorld {
    /// Builds from the compiled VS-001 extract without network/filesystem I/O.
    /// IDs reuse ADR 0016's building purpose and `[dataset_version, BIN]`
    /// integer address. A BIN is not an ordinal in the source file.
    ///
    /// # Errors
    /// Invalid/duplicate source rows or a collision; no partial world escapes.
    pub fn new(seed: u64) -> Result<Self, SliceError> {
        Self::load(seed, BLOCK_BUILDINGS, |bin| {
            ids::building_id(seed, DATASET_VERSION, bin)
        })
    }

    fn load(
        seed: u64,
        source: &[SourceBuilding],
        id_for: impl Fn(u32) -> EntityId,
    ) -> Result<Self, SliceError> {
        let mut bins = BTreeSet::new();
        let mut rows = Vec::new();
        for building in source {
            if !bins.insert(building.bin) {
                return Err(SliceError::DuplicateBin(building.bin));
            }
            if building.height_mm == 0
                || !building.centroid.valid()
                || !building.footprint.is_valid()
            {
                return Err(SliceError::InvalidBuilding(building.bin));
            }
            if building.featured.is_some() {
                rows.push((id_for(building.bin), *building));
            }
        }
        let mut buildings = Column::new();
        buildings
            .insert_batch(rows)
            .map_err(|e| SliceError::Collision(e.0))?;
        Ok(Self {
            baseline: Baseline {
                seed,
                dataset_version: DATASET_VERSION,
                generator_version: GENERATOR_VERSION,
            },
            buildings,
        })
    }

    /// Inputs retained for future slice persistence; no save codec is changed.
    #[must_use]
    pub const fn baseline(&self) -> Baseline {
        self.baseline
    }

    /// Detailed buildings, in canonical EntityId order, with full source data.
    #[must_use]
    pub const fn buildings(&self) -> &Column<SourceBuilding> {
        &self.buildings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footprint_validation_retains_parts_holes_winding_and_closure() {
        const A: GeoPoint = GeoPoint {
            lon_pdeg: 0,
            lat_pdeg: 0,
        };
        const B: GeoPoint = GeoPoint {
            lon_pdeg: 10,
            lat_pdeg: 0,
        };
        const C: GeoPoint = GeoPoint {
            lon_pdeg: 10,
            lat_pdeg: 10,
        };
        const OUTER: Ring = Ring {
            points: &[A, B, C, A],
        };
        const HOLE: Ring = Ring {
            points: &[A, C, B, A],
        };
        let footprint = Footprint {
            polygons: &[
                Polygon {
                    rings: &[OUTER, HOLE],
                },
                Polygon { rings: &[OUTER] },
            ],
        };
        assert!(footprint.is_valid());
        assert_eq!(footprint.polygons.len(), 2);
        assert_eq!(footprint.polygons[0].rings[1].points, &[A, C, B, A]);
        for invalid in [
            Footprint { polygons: &[] },
            Footprint {
                polygons: &[Polygon { rings: &[] }],
            },
            Footprint {
                polygons: &[Polygon {
                    rings: &[Ring { points: &[A, B, C] }],
                }],
            },
            Footprint {
                polygons: &[Polygon {
                    rings: &[Ring {
                        points: &[A, A, A, A],
                    }],
                }],
            },
            Footprint {
                polygons: &[Polygon {
                    rings: &[OUTER, Ring { points: &[] }],
                }],
            },
        ] {
            assert!(!invalid.is_valid());
        }
    }

    #[test]
    fn source_order_cannot_change_ids_or_components() {
        let mut reversed = BLOCK_BUILDINGS.to_vec();
        reversed.reverse();
        for seed in [0, 42, u64::MAX] {
            assert_eq!(
                SliceWorld::new(seed).unwrap(),
                SliceWorld::load(seed, &reversed, |bin| ids::building_id(
                    seed,
                    DATASET_VERSION,
                    bin
                ))
                .unwrap()
            );
        }
    }

    #[test]
    fn duplicate_source_keys_fail_before_hashing() {
        let row = BLOCK_BUILDINGS[0];
        assert_eq!(
            SliceWorld::load(42, &[row, row], |_| unreachable!()),
            Err(SliceError::DuplicateBin(row.bin))
        );
    }

    #[test]
    fn distinct_addresses_that_collide_are_rejected_without_renumbering() {
        let id = EntityId::procedural(0);
        assert_eq!(
            SliceWorld::load(42, BLOCK_BUILDINGS, |_| id),
            Err(SliceError::Collision(id))
        );
        assert_eq!(SliceWorld::new(42).unwrap().buildings().len(), 4);
    }

    #[test]
    fn invalid_geometry_or_height_never_constructs_a_world() {
        let mut row = BLOCK_BUILDINGS[0];
        row.height_mm = 0;
        assert_eq!(
            SliceWorld::load(0, &[row], |_| unreachable!()),
            Err(SliceError::InvalidBuilding(row.bin))
        );
        row = BLOCK_BUILDINGS[0];
        row.footprint = Footprint { polygons: &[] };
        assert_eq!(
            SliceWorld::load(0, &[row], |_| unreachable!()),
            Err(SliceError::InvalidBuilding(row.bin))
        );
    }
}
