//! The fixed, versioned toy "dataset": a few box buildings at fixed
//! offsets from the Bay Ridge reference point (#7). Not real data.

use ws_math::reference::BAY_RIDGE;
use ws_math::{CoordError, EnuFrame, Position, SurfacePos};

/// Version of the built-in toy dataset. It is an input to every procedural
/// ID and decision, so changing [`TOY_BUILDINGS`] means bumping it and
/// updating the Tier 1 golden values with Product Owner approval.
pub const TOY_DATASET_VERSION: u64 = 1;

/// One building of the toy dataset, in metres in the ENU frame at
/// [`BAY_RIDGE`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToyBuildingSpec {
    /// Metres east of the reference point.
    pub east_m: f64,
    /// Metres north of the reference point.
    pub north_m: f64,
    /// Half the side of the square footprint, in metres.
    pub half_extent_m: f32,
    /// Height of the box, in metres.
    pub height_m: f32,
}

/// The toy dataset: five boxes spread over about 1 km, far enough apart
/// that walks between them cross sector-cell edges.
pub const TOY_BUILDINGS: [ToyBuildingSpec; 5] = [
    ToyBuildingSpec {
        east_m: 0.0,
        north_m: 0.0,
        half_extent_m: 10.0,
        height_m: 12.0,
    },
    ToyBuildingSpec {
        east_m: 460.0,
        north_m: 140.0,
        half_extent_m: 8.0,
        height_m: 9.0,
    },
    ToyBuildingSpec {
        east_m: -380.0,
        north_m: 310.0,
        half_extent_m: 12.0,
        height_m: 15.0,
    },
    ToyBuildingSpec {
        east_m: 150.0,
        north_m: -520.0,
        half_extent_m: 9.0,
        height_m: 10.0,
    },
    ToyBuildingSpec {
        east_m: -270.0,
        north_m: -340.0,
        half_extent_m: 11.0,
        height_m: 21.0,
    },
];

/// Where one building of a dataset stands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuildingSite {
    /// Ground-level centre of the footprint.
    pub pos: SurfacePos,
    /// Half the side of the square footprint, in metres.
    pub half_extent_m: f32,
    /// Height of the box, in metres.
    pub height_m: f32,
}

/// A versioned set of building sites. A site's index in `sites` is its
/// building index, an input to procedural IDs.
#[derive(Debug, Clone, PartialEq)]
pub struct Dataset {
    /// Dataset version, an input to every procedural ID and decision.
    pub version: u64,
    /// Building sites, by building index.
    pub sites: Vec<BuildingSite>,
}

impl Dataset {
    /// The built-in toy dataset ([`TOY_BUILDINGS`], version
    /// [`TOY_DATASET_VERSION`]).
    ///
    /// # Errors
    ///
    /// Only as [`SurfacePos::from_ecef`]; never for the built-in values.
    pub fn toy() -> Result<Self, CoordError> {
        let frame = EnuFrame::new(BAY_RIDGE);
        let sites = TOY_BUILDINGS
            .iter()
            .map(|b| {
                let p = frame.to_ecef(Position::new(b.east_m, b.north_m, 0.0));
                Ok(BuildingSite {
                    pos: SurfacePos::from_ecef(p)?,
                    half_extent_m: b.half_extent_m,
                    height_m: b.height_m,
                })
            })
            .collect::<Result<_, CoordError>>()?;
        Ok(Self {
            version: TOY_DATASET_VERSION,
            sites,
        })
    }
}
