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
/// [`SurfacePos::from_ecef_in`]. Movement also shares it with
/// [`SurfacePos::reexpress_with`]. Converting with a frame does not allocate.
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
/// Every position produced by [`SurfacePos::reexpress`] or
/// [`SurfacePos::from_ecef`] lies in its own cell:
/// `p.occupied_cell() == Ok(p.cell())`, also within a fraction of a
/// millimetre of a cell edge, where rounding the offset to `f32` could
/// otherwise put it back across the edge (ADR 0016 §4). Re-expression is
/// therefore idempotent, and a position that does not move never changes
/// cell. [`SurfacePos::new`] and [`SurfacePos::from_ecef_in`] accept offsets
/// outside the cell, which is how movement writes positions between
/// re-expressions.
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
    /// it, and its offset in that cell's frame, chosen so that the stored
    /// position lies in that cell (ADR 0016 §4). The cell decision is Tier 1.
    ///
    /// # Errors
    ///
    /// As [`Geodetic::from_ecef`](crate::wgs84::Geodetic::from_ecef), and
    /// [`CoordError::BoundaryUnresolved`] as described there.
    pub fn from_ecef(p: Position<Ecef>) -> Result<Self, CoordError> {
        if !p.is_finite() {
            return Err(CoordError::NonFinite);
        }
        let cell = CellId::from_ecef(p)?.parent(SECTOR_LEVEL)?;
        settle(p, &CellFrame::new(cell))
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

    /// Re-expresses the position in the sector cell that contains it
    /// (ADR 0016 §4).
    ///
    /// 1. Convert to the 64-bit ECEF point `P` and find the sector cell `T`
    ///    containing it.
    /// 2. If `T` is the position's cell, return the position unchanged, bit
    ///    for bit.
    /// 3. Otherwise express `P` in `T`'s frame, rounding the offset to the
    ///    nearest `f32`. If that lies in `T`, return it.
    /// 4. Otherwise (rounding put it back across the edge), among the 3 × 3
    ///    neighbours of that offset on the `f32` grid (east and north each
    ///    stepped by −1, 0, or +1 representable values; up unchanged), return
    ///    the one lying in `T` whose ECEF point is nearest to `P`, ties going
    ///    to the first in east-then-north order from −1 to +1; if none lies
    ///    in `T`, the same over 5 × 5.
    ///
    /// The result always lies in its own cell, so calling this again returns
    /// it unchanged. Every step uses only `libm`, bit-level `f32` steps, and
    /// correctly rounded arithmetic, so the result, including the cell, is
    /// the same on every platform (Tier 1). Wherever step 3 already gives a
    /// position inside `T`, the result is exactly what it was before ADR
    /// 0016. Simulation code calls this at a fixed point in the tick
    /// (07-coordinates-and-time.md §8.3).
    ///
    /// # Errors
    ///
    /// As [`CellId::from_ecef`], and [`CoordError::BoundaryUnresolved`] if
    /// step 4 finds no candidate (never observed).
    pub fn reexpress(&self) -> Result<Self, CoordError> {
        let p = self.to_ecef();
        let cell = CellId::from_ecef(p)?.parent(SECTOR_LEVEL)?;
        if cell == self.cell {
            Ok(*self)
        } else {
            settle(p, &CellFrame::new(cell))
        }
    }

    /// Re-expresses with a prebuilt frame of this position's cell.
    ///
    /// Exactly the same cell lookup and ADR 0016 edge settling as
    /// [`Self::reexpress`], without rebuilding the source frame. Reuse one
    /// frame for all moving agents in a cell. This is not an approximate
    /// inside test: points near an edge still use the full Tier 1 lookup.
    ///
    /// # Errors
    ///
    /// [`CoordError::FrameMismatch`] for a frame of another cell; otherwise
    /// the same errors as [`Self::reexpress`].
    pub fn reexpress_with(&self, frame: &CellFrame) -> Result<Self, CoordError> {
        let p = self.to_ecef_with(frame)?;
        let cell = CellId::from_ecef(p)?.parent(SECTOR_LEVEL)?;
        if cell == self.cell {
            Ok(*self)
        } else {
            settle(p, &CellFrame::new(cell))
        }
    }

    /// Whether the stored position lies in its own cell, with a prebuilt
    /// frame of that cell.
    fn lies_in_own_cell(&self, frame: &CellFrame) -> Result<bool, CoordError> {
        let p = self.to_ecef_with(frame)?;
        Ok(CellId::from_ecef(p)?.parent(SECTOR_LEVEL)? == self.cell)
    }
}

/// Expresses `p` in `frame`'s cell so that the stored position lies in that
/// cell: steps 3 and 4 of [`SurfacePos::reexpress`] (ADR 0016 §4).
fn settle(p: Position<Ecef>, frame: &CellFrame) -> Result<SurfacePos, CoordError> {
    let rounded = SurfacePos::from_ecef_in(p, frame)?;
    if rounded.lies_in_own_cell(frame)? {
        return Ok(rounded);
    }
    let [e, n, u] = rounded.offset;
    for radius in 1..=2 {
        let mut best: Option<(f64, SurfacePos)> = None;
        for de in -radius..=radius {
            for dn in -radius..=radius {
                let offset = [f32_steps(e, de), f32_steps(n, dn), u];
                let Ok(candidate) = SurfacePos::new(frame.cell, offset) else {
                    continue;
                };
                if !candidate.lies_in_own_cell(frame)? {
                    continue;
                }
                let d = candidate.to_ecef_with(frame)? - p;
                let d2 = d.dot(d);
                // Strictly smaller, so ties keep the first in scan order.
                if best.is_none_or(|(b, _)| d2 < b) {
                    best = Some((d2, candidate));
                }
            }
        }
        if let Some((_, pos)) = best {
            return Ok(pos);
        }
    }
    Err(CoordError::BoundaryUnresolved)
}

/// `v` moved by `steps` representable `f32` values (up for positive steps).
/// Bit-level, so exact on every platform.
fn f32_steps(mut v: f32, steps: i32) -> f32 {
    for _ in 0..steps.unsigned_abs() {
        v = if steps > 0 {
            v.next_up()
        } else {
            v.next_down()
        };
    }
    v
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
        assert_eq!(p.reexpress_with(&other), Err(CoordError::FrameMismatch));
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

    /// The input ADR 0016 §4 documents. Its 64-bit point lies about 30 µm
    /// inside `89c24564…`, but rounding the offset to `f32` in that cell's
    /// frame puts it back in `89c2455c…`, so before ADR 0016 re-expression
    /// alternated between the two cells on every call.
    fn adr_0016_case() -> SurfacePos {
        let cell = CellId::from_raw(0x89c2_455c_0000_0000).unwrap();
        let offset = [
            f32::from_bits(0xc29b_4918),
            f32::from_bits(0xc405_dcb4),
            0.0,
        ];
        SurfacePos::new(cell, offset).unwrap()
    }

    #[test]
    fn adr_0016_case_needs_the_edge_rule() {
        // Guards that the case still exercises step 4: plain rounding in the
        // target cell's frame lands outside that cell.
        let p = adr_0016_case();
        let target = CellId::from_raw(0x89c2_4564_0000_0000).unwrap();
        assert_eq!(p.occupied_cell(), Ok(target));
        let rounded = SurfacePos::from_ecef_in(p.to_ecef(), &CellFrame::new(target)).unwrap();
        assert_eq!(
            rounded.occupied_cell(),
            Ok(p.cell()),
            "rounding crosses back"
        );
    }

    #[test]
    fn adr_0016_case_settles_in_the_containing_cell() {
        let p = adr_0016_case();
        let q = p.reexpress().unwrap();
        assert_eq!(p.reexpress_with(&CellFrame::new(p.cell())), Ok(q));
        assert_eq!(q.cell(), CellId::from_raw(0x89c2_4564_0000_0000).unwrap());
        assert_eq!(q.occupied_cell(), Ok(q.cell()));
        assert!(q.to_ecef().distance(p.to_ecef()) < 1e-4);
        let mut r = q;
        for _ in 0..20 {
            r = r.reexpress().unwrap();
            assert_eq!(r, q, "repeated re-expression does not move it");
        }
    }

    #[test]
    fn from_ecef_also_lands_inside_its_cell() {
        let p = adr_0016_case().to_ecef();
        let q = SurfacePos::from_ecef(p).unwrap();
        assert_eq!(q.occupied_cell(), Ok(q.cell()));
        assert_eq!(q, adr_0016_case().reexpress().unwrap());
        assert_eq!(
            SurfacePos::from_ecef(Position::new(f64::NAN, 0.0, 0.0)),
            Err(CoordError::NonFinite)
        );
    }

    #[test]
    fn f32_steps_move_by_representable_values() {
        assert_eq!(f32_steps(1.0, 0), 1.0);
        assert_eq!(f32_steps(1.0, 1), f32::from_bits(1.0f32.to_bits() + 1));
        assert_eq!(f32_steps(1.0, -1), f32::from_bits(1.0f32.to_bits() - 1));
        assert_eq!(f32_steps(-1.0, 1), f32::from_bits((-1.0f32).to_bits() - 1));
        assert_eq!(f32_steps(-1.0, -2), f32::from_bits((-1.0f32).to_bits() + 2));
        assert!(f32_steps(0.0, 1) > 0.0 && f32_steps(0.0, -1) < 0.0);
    }
}
