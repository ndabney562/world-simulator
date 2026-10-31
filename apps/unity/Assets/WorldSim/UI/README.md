# UI

The game HUD, built with UI Toolkit (`docs/architecture/03-rendering-architecture.md` §4.3).

- `GameHud.cs`: time controls, save/load, resident card, event feed and power bar. The bar has four powers: Curse with illness and Bless (select, then tap a resident), Gathering (select, then tap a building; 30 minutes) and Storm (casts at once for one hour). A wrong tap shows a 3-second warning. Every cast adds a watch carrying the root's tag, polled once a second: illness (13) through the paired path, bless (14), storm (16) and gathering (20) through the root-only `Consequences(root)`; a rejected cast is shown as a warning and creates no watch. It sits next to `SliceDemo` in the RealBlock scene and calls only SliceDemo's existing actions. It never changes simulation state directly.
- `HudText.cs`: pure text for the feed and the card. Editor tests cover every event tag the feed shows (5-6, 10-22) and when each root group is finished.

`WorldSim.UI.asmref` compiles this folder into `WorldSim.Rendering`, so the agreed three-assembly layout is unchanged.
