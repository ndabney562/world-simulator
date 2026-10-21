//! ADR0031 pure real-slice meshes, resident/count snapshots and camera adapters.
use super::{Operation, SliceError, SliceSimulation};
use crate::{CalendarSky, EntityId, SimTime};
pub use ws_life::lod::{BuildingSummary, Summary};
use ws_life::{
    behavior::{BehaviorError, FineLocation},
    lod::{CameraInterest, LodError, governing},
};
pub use ws_world::slice_render::{
    Geometry, MAX_BUILDINGS, MAX_INDICES, MAX_VERTICES, RenderError, VERSION as RENDER_VERSION,
};

/// Fixed frame latitude/longitude degrees and explicitly ellipsoidal height metres.
#[must_use]
pub fn frame_origin() -> [f64; 3] {
    let p = ws_world::environment::SOLAR_SITE;
    [p.lat_pdeg as f64 / 1e12, p.lon_pdeg as f64 / 1e12, 0.0]
}

/// One retained identity; geometry exists only for an actual local fine view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Resident {
    /// Existing stable resident identity.
    pub id: EntityId,
    /// Canonical controlling building, separate from physical occupancy.
    pub governing: EntityId,
    /// Fixed-frame east/north/up metres, zero unless location1.
    pub position: [f32; 3],
    /// Zero-east, positive-north radians, stationary positive zero.
    pub heading: f32,
    /// Sole existing detail tag1/2.
    pub detail: u32,
    /// Existing logical activity tag0–5.
    pub activity: u32,
    /// 0no fine position,1local fine,2off-map transit,3stationary off-map.
    pub location: u32,
}
/// Immutable bulk output from one committed boundary, without query-side effects.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    /// Physical ticks committed.
    pub tick: u64,
    /// Committed uniform-SI instant.
    pub time: SimTime,
    /// Canonical hash at this boundary.
    pub state_hash: u64,
    /// Existing dataset version for static/dynamic/sky coherence.
    pub dataset: u64,
    /// Committed0/1/10/100 pacing scale.
    pub scale: u32,
    /// Exact existing counts and four building summaries.
    pub summary: Summary,
    /// All retained residents in ascending stable-ID order.
    pub residents: Vec<Resident>,
}
impl SliceSimulation {
    /// Construct the immutable complete source mesh once for a native handle.
    /// # Errors
    /// Unsupported source/topology, conversion or allocation failure.
    pub fn render_geometry(&self) -> Result<Geometry, RenderError> {
        Geometry::new(self.population().world())
    }
    /// Coherent pure calendar/sky in exactly the real slice geometry's ENU basis.
    #[must_use]
    pub fn calendar_sky(&self) -> CalendarSky {
        CalendarSky {
            tick: self.clock().steps(),
            time: self.clock().now(),
            state_hash: self.state_hash(),
            scale_tag: self.clock().scale().multiplier(),
            environment: ws_world::environment::calendar_sky(
                self.clock().now(),
                Some(ws_world::environment::site_frame()),
            ),
        }
    }
    /// Normalize a coarse presentation request once into replayable integer input.
    /// Does not apply it or change any canonical quantity.
    /// # Errors
    /// Nonfinite/negative/excessive radius or invalid frame/interest coordinates.
    pub fn camera_operation(
        &self,
        target: [f64; 3],
        radius_m: f64,
    ) -> Result<Operation, SliceError> {
        if !radius_m.is_finite() || !(0.0..=1_000_000.0).contains(&radius_m) {
            return Err(SliceError::Lod(LodError::Input));
        }
        let center = self
            .behavior()
            .navigation()
            .from_render(target)
            .map_err(|e| SliceError::Behavior(BehaviorError::Navigation(e)))?;
        let radius_mm = (radius_m * 1000.0).ceil() as u64;
        let camera = CameraInterest { center, radius_mm };
        camera.validate().map_err(SliceError::Lod)?;
        Ok(Operation::CameraInterest(camera))
    }
    /// Bulk resident geometry/count view. No stepping, RNG, materialization or log writes.
    /// Temporary allocation is bounded by the existing30-person cohort.
    /// # Errors
    /// Failed domain view, output reservation or frame conversion.
    pub fn render_snapshot(&self) -> Result<Snapshot, SliceError> {
        let view = self.view()?;
        let mut residents = Vec::new();
        residents
            .try_reserve(view.residents.len())
            .map_err(|_| SliceError::Resource)?;
        for s in view.residents {
            let home = self
                .population()
                .residents()
                .get(s.resident)
                .ok_or(SliceError::Lod(LodError::Invariant))?
                .home;
            let (location, position, heading) = match s.fine {
                None => (0, [0.0; 3], 0.0),
                Some(FineLocation::OffMapTransit { .. }) => (2, [0.0; 3], 0.0),
                Some(FineLocation::OffMap { .. }) => (3, [0.0; 3], 0.0),
                Some(FineLocation::Local { point, walking }) => {
                    let position = self
                        .behavior()
                        .navigation()
                        .to_render(point)
                        .map_err(|e| SliceError::Behavior(BehaviorError::Navigation(e)))?
                        .map(|v| v as f32);
                    let heading = if walking {
                        self.behavior()
                            .render_heading(s.resident, view.time)
                            .map_err(SliceError::Behavior)?
                    } else {
                        0.0
                    };
                    (1, position, heading)
                }
            };
            residents.push(Resident {
                id: s.resident,
                governing: governing(s.logical.location, home),
                position,
                heading,
                detail: s.detail as u32,
                activity: s.logical.activity as u32,
                location,
            });
        }
        Ok(Snapshot {
            tick: self.clock().steps(),
            time: view.time,
            state_hash: self.state_hash(),
            dataset: ws_world::slice::DATASET_VERSION,
            scale: self.clock().scale().multiplier(),
            summary: view.summary,
            residents,
        })
    }
}

#[cfg(test)]
#[path = "slice_render/tests.rs"]
mod tests;
