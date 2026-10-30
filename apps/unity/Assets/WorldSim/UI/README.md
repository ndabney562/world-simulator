# UI

The game HUD, built with UI Toolkit (`docs/architecture/03-rendering-architecture.md` §4.3).

- `GameHud.cs`: time controls, save/load, resident card, event feed and power bar. It sits next to `SliceDemo` in the RealBlock scene and calls only SliceDemo's existing actions. It never changes simulation state directly.
- `HudText.cs`: pure text for the feed and the card. Editor tests cover every event tag the feed shows.

`WorldSim.UI.asmref` compiles this folder into `WorldSim.Rendering`, so the agreed three-assembly layout is unchanged.
