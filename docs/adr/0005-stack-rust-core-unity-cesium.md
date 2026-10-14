# ADR 0005: Stack is a Rust core with a Unity 6 (URP) and Cesium for Unity front end

- Status: Accepted
- Date: 2026-10-14
- Decided by: Noah (accepting the revised recommendation)

## Context
The original recommendation was a Rust core plus a custom Swift/Metal renderer. ADRs 0001 and 0002 (real Earth, realistic visuals) made a mature engine with a geospatial 3D Tiles client the better front end.

## Decision
- **Simulation:** a headless, deterministic Rust core (Cargo workspace), built and tested on Linux and macOS, and shipped to iOS as a native static library.
- **Front end:** Unity 6 using the Universal Render Pipeline, with Cesium for Unity for georeferencing (WGS84/ECEF with origin shifting) and 3D Tiles streaming.
- **Boundary:** commands go into the core and render snapshots come out, over a coarse-grained, buffer-based C ABI. Unity never owns simulation state.

## Consequences
- Reproducibility lives entirely in the Rust core (ADR 0010); Unity's floating-point behaviour cannot affect simulation results.
- The first spikes: the Rust static library as a Unity iOS native plugin; Cesium plus NYC open data rendering on an iPhone 15 Pro (Technical Prototype, ADR 0009).
- Unity Personal licence terms apply (free below a revenue threshold).
