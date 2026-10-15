//! Surface positions: a sector cell plus a 32-bit offset in that cell's
//! ENU frame (07-coordinates-and-time.md §8.3).

use crate::cell::CellId;
use crate::enu::EnuFrame;
use crate::error::CoordError;
use crate::frame::{Ecef, Enu, Position};

/// The cell level used for sector cells, the unit of simulation
/// partitioning, materialization, streaming, and persistence. Level 13
/// cells are about 1 km across (0.6 to 1.2 km depending on where they lie
/// on the cube face). This is the proposed default from
/// 21-decisions-now-vs-later.md S-4 and ADR 0015; it is confirmed or
/// changed once materialization costs are measured.
pub const SECTOR_LEVEL: u8 = 13;

/// A sector cell together with its precomputed ENU frame.
///
/// Building a cell's frame costs a few trigonometric calls, so code that
/// converts many positions in the same cell (snapshot building) builds the
/// `CellFrame` once and passes it to [`SurfacePos::to_ecef_with`] and
/// [`SurfacePos::from_ecef_in`]. Converting with a frame does not
/// allocate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellFrame {
    cell: CellId,
    frame: EnuFrame,
}

impl CellFrame {
    /// Builds the frame of `cell`.
    #[must_use]
    pub fn new(cell: CellId) -> Self {
        Self {
            cell,
            frame: cell.enu_frame(),
        }
    }

    /// The cell this frame belongs to.
    #[must_use]
    pub const fn cell(&self) -> CellId {
        self.cell
    }

    /// The cell's ENU frame.
    #[must_use]
    pub const fn frame(&self) -> &EnuFrame {
        &self.frame
    }
}

/// A position on or near the Earth's surface: a sector cell and an offset
/// in metres in that cell's ENU frame.
///
/// The offset is 32-bit. Within a sector cell (offsets up to about 1 km)
/// its spacing is under 0.06 mm. Vertical spacing grows with height above
/// the ellipsoid (about 0.5 mm at 8 km), so positions far above the
/// surface, such as aircraft or spacecraft, belong in 64-bit ECEF instead.
///
/// The cell is authoritative: which cell an entity is in is a Tier 1
/// integer fact. When movement takes the offset outside the cell,
/// [`SurfacePos::reexpress`] moves the position into the cell that now
/// contains it, deterministically.
///
/// Within a fraction of a millimetre of a cell edge, rounding the offset to
/// `f32` can leave the stored position just outside its `cell`, so
/// [`SurfacePos::occupied_cell`] can differ from [`SurfacePos::cell`] and
/// repeated re-expression can alternate between two cells (ADR 0015,
/// Consequences). Movement systems (TP-006) decide the remedy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfacePos {
    cell: CellId,
    offset: [f32; 3],
}

impl SurfacePos {
    /// Creates a surface position from a sector cell and an ENU offset in
    /// metres. The offset may lie outside the cell; see
    /// [`SurfacePos::reexpress`].
    ///
    /// # Errors
    ///
    /// [`CoordError::NotSectorCell`] if `cell` is not at [`SECTOR_LEVEL`],
    /// and [`CoordError::NonFinite`] if an offset component is NaN or
    /// infinite.
    pub fn new(cell: CellId, offset: [f32; 3]) -> Result<Self, CoordError> {
        if cell.level() != SECTOR_LEVEL {
            return Err(CoordError::NotSectorCell);
        }
        if !offset.iter().all(|c| c.is_finite()) {
            return Err(CoordError::NonFinite);
        }
        Ok(Self { cell, offset })
    }

    /// The sector cell whose frame the offset is expressed in.
    #[must_use]
    pub const fn cell(&self) -> CellId {
        self.cell
    }

    /// The offset in metres (east, north, up) in the cell's ENU frame.
    #[must_use]
    pub const fn offset(&self) -> [f32; 3] {
        self.offset
    }

    /// The offset as a 64-bit position in the cell's ENU frame.
    #[must_use]
    pub fn offset_enu(&self) -> Position<Enu> {
        let [e, n, u] = self.offset;
        Position::new(f64::from(e), f64::from(n), f64::from(u))
    }

    /// The surface position of an ECEF point: the sector cell containing
    /// it, and its offset in that cell's frame. The cell decision is Tier 1.
    ///
    /// # Errors
    ///
    /// As [`Geodetic::from_ecef`](crate::wgs84::Geodetic::from_ecef).
    pub fn from_ecef(p: Position<Ecef>) -> Result<Self, CoordError> {
        let cell = CellId::from_ecef(p)?.parent(crate::surface::SECTOR_LEVEL)?;
        Self::from_ecef_in(p, &CellFrame::new(cell))
    }

    /// Expresses an ECEF point in a given sector cell's frame, whether or
    /// not the point lies inside that cell.
    ///
    /// # Errors
    ///
    /// [`CoordError::NotSectorCell`] if the frame's cell is not at
    /// [`SECTOR_LEVEL`], and [`CoordError::NonFinite`] if `p` is not finite
    /// or the offset does not fit in `f32`.
    pub fn from_ecef_in(p: Position<Ecef>, frame: &CellFrame) -> Result<Self, CoordError> {
        if !p.is_finite() {
            return Err(CoordError::NonFinite);
        }
        let local = frame.frame.to_enu(p);
        // `as` rounds to nearest, which is deterministic.
        Self::new(frame.cell, [local.x as f32, local.y as f32, local.z as f32])
    }

    /// Converts to ECEF, building the cell's frame. For many positions in
    /// one cell, use [`SurfacePos::to_ecef_with`].
    #[must_use]
    pub fn to_ecef(&self) -> Position<Ecef> {
        self.cell.enu_frame().to_ecef(self.offset_enu())
    }

    /// Converts to ECEF with a prebuilt frame. Allocation-free.
    ///
    /// # Errors
    ///
    /// [`CoordError::FrameMismatch`] if `frame` belongs to another cell.
    pub fn to_ecef_with(&self, frame: &CellFrame) -> Result<Position<Ecef>, CoordError> {
        if frame.cell != self.cell {
            return Err(CoordError::FrameMismatch);
        }
        Ok(frame.frame.to_ecef(self.offset_enu()))
    }

    /// The sector cell the position actually lies in, which differs from
    /// [`SurfacePos::cell`] once the offset has left the cell. Tier 1.
    ///
    /// # Errors
    ///
    /// As [`CellId::from_ecef`].
    pub fn occupied_cell(&self) -> Result<CellId, CoordError> {
        CellId::from_ecef(self.to_ecef())?.parent(SECTOR_LEVEL)
    }

    /// Re-expresses the position in the sector cell that contains it.
    ///
    /// If the offset is still inside its cell, the position is returned
    /// unchanged, bit for bit. Otherwise the position is converted to ECEF
    /// and expressed in the containing cell's frame. Every step uses only
    /// `libm` and correctly rounded arithmetic, so the result, including
    /// the new cell, is the same on every platform. Simulation code calls
    /// this at a fixed point in the tick (07-coordinates-and-time.md §8.3).
    ///
    /// Not guaranteed idempotent within a fraction of a millimetre of a cell
    /// edge: the `f32` offset in the new cell can round back across the
    /// edge, and calling this again then moves it back (see the type docs).
    ///
    /// # Errors
    ///
    /// As [`CellId::from_ecef`].
    pub fn reexpress(&self) -> Result<Self, CoordError> {
        let p = self.to_ecef();
        let cell = CellId::from_ecef(p)?.parent(SECTOR_LEVEL)?;
        if cell == self.cell {
            Ok(*self)
        } else {
            Self::from_ecef_in(p, &CellFrame::new(cell))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reference::BAY_RIDGE;

    fn bay_ridge_sector() -> CellId {
        CellId::from_geodetic_at_level(&BAY_RIDGE, SECTOR_LEVEL).unwrap()
    }

    #[test]
    fn new_validates_level_and_finiteness() {
        let sector = bay_ridge_sector();
        assert!(SurfacePos::new(sector, [1.0, 2.0, 3.0]).is_ok());
        assert_eq!(
            SurfacePos::new(sector.parent(12).unwrap(), [0.0; 3]),
            Err(CoordError::NotSectorCell)
        );
        assert_eq!(
            SurfacePos::new(sector, [f32::NAN, 0.0, 0.0]),
            Err(CoordError::NonFinite)
        );
        assert_eq!(
            SurfacePos::new(sector, [0.0, f32::INFINITY, 0.0]),
            Err(CoordError::NonFinite)
        );
    }

    #[test]
    fn cell_centre_is_the_frame_origin() {
        let sector = bay_ridge_sector();
        let p = SurfacePos::new(sector, [0.0; 3]).unwrap();
        let centre = sector.center_geodetic().to_ecef();
        assert_eq!(p.to_ecef(), centre);
        assert_eq!(p.occupied_cell(), Ok(sector));
    }

    #[test]
    fn frame_mismatch_is_rejected() {
        let sector = bay_ridge_sector();
        let other = CellFrame::new(sector.edge_neighbors()[0]);
        let p = SurfacePos::new(sector, [0.0; 3]).unwrap();
        assert_eq!(p.to_ecef_with(&other), Err(CoordError::FrameMismatch));
        assert_eq!(p.to_ecef_with(&CellFrame::new(sector)), Ok(p.to_ecef()));
    }

    #[test]
    fn reexpress_inside_the_cell_is_the_identity() {
        let p = SurfacePos::new(bay_ridge_sector(), [10.5, -20.25, 3.0]).unwrap();
        assert_eq!(p.reexpress(), Ok(p));
    }

    #[test]
    fn reexpress_moves_into_the_neighbouring_cell() {
        let sector = bay_ridge_sector();
        // 3 km east is outside any level-13 cell's half-width.
        let p = SurfacePos::new(sector, [3000.0, 0.0, 0.0]).unwrap();
        let q = p.reexpress().unwrap();
        assert_ne!(q.cell(), sector);
        assert_eq!(q.cell().level(), SECTOR_LEVEL);
        assert_eq!(q.occupied_cell(), Ok(q.cell()));
        assert!(q.to_ecef().distance(p.to_ecef()) < 1e-4);
        assert_eq!(q.reexpress(), Ok(q), "re-expressing again is stable");
    }
}
