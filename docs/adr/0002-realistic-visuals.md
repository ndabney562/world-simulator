# ADR 0002: Realistic visuals preferred

- Status: Accepted (refined by ADRs 0007 and 0008: the Google backdrop is optional and capped; open-data visuals are the default)
- Date: 2026-10-14
- Decided by: Noah

## Context
The original proposal recommended a stylized look so that a custom Metal renderer would be tractable on iPhone (assumption A6). Noah prefers realistic visuals.

## Decision
Target a realistic look through a hybrid approach: Google Photorealistic 3D Tiles from orbit down to the neighbourhood band, and realistic PBR geometry generated from open data near the camera and wherever the world has been changed. Simulation depth and sustained frame rate still take priority over visual fidelity when they conflict.

## Consequences
- The front-end recommendation changes from custom Swift/Metal to Unity (URP) plus Cesium for Unity. The Rust simulation core is unchanged.
- The seam between Google geometry and our own geometry is a risk to evaluate in the Technical Prototype and Vertical Slice.
- Street-level crowds still rely on LOD: a few dozen realistic characters near the camera, with simpler representations beyond.
