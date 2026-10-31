# UI

The game HUD, built with UI Toolkit (`docs/architecture/03-rendering-architecture.md` §4.3).

- `GameHud.cs`: time controls, save/load, resident card, event feed and power bar. The bar has four powers: Curse with illness and Bless (select, then tap a resident), Gathering (select, then tap a building; 30 minutes) and Storm (casts at once for one hour). A wrong tap shows a 3-second warning. Every cast adds a watch carrying the root's tag, polled once a second: illness (13) through the paired path, bless (14), storm (16) and gathering (20) through the root-only `Consequences(root)`; a rejected cast creates no watch, keeps the armed power, and shows "That didn't work. Try another target." (never the bridge's text); while a storm or gathering watch is unfinished, Storm and Gathering are refused with "A storm or gathering is already under way.". The decisions live in the pure `HudCasts.cs`. It sits next to `SliceDemo` in the RealBlock scene and calls only SliceDemo's existing actions. It never changes simulation state directly.
- `HudText.cs`: pure text for the feed and the card. Editor tests cover every event tag the feed shows (5-6, 10-22) and when each root group is finished: illness at its recovery or its scheduled end (a bless cure adds no recovery row), a gathering once ended and every joiner is home or 30 minutes after its end.

`WorldSim.UI.asmref` compiles this folder into `WorldSim.Rendering`, so the agreed three-assembly layout is unchanged.
