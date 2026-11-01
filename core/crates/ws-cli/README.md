# ws-cli

Responsibility: a headless host for the Vertical Slice: run it, save it and load it. Workspace layer 6, alongside other adapters; it depends on `ws-sim` and `ws-persist`, and nothing depends on it. It uses only the standard library plus internal crates at runtime (and, through `ws-persist`, bundled SQLite). Domain crates keep their no-filesystem/environment/clock rule.

Owned state: CLI arguments, the slice simulation and file handles. It owns no separate simulation state and emits no domain events itself. Since [ADR 0040](../../../docs/adr/0040-delete-toy-world-and-old-formats.md) the slice is the only model: the toy world, its `--journal` debug tape and `replay` mode are deleted.

From `core/`:

```sh
cargo run --locked -p ws-cli -- run --seed 42 --ticks 2000 --hash-every 200 --script crates/ws-cli/tests/fixtures/ci-slice.txt
cargo run --locked -p ws-cli -- save --seed 7 --ticks 800 --out /tmp/slice.wssave
cargo run --locked -p ws-cli -- load --file /tmp/slice.wssave --ticks 100 --hash-every 10
```

`run` creates a calendar-aligned slice for the seed, applies the script, then steps until `--ticks` physical ticks have run, printing `tick=N state=HASH` every `--hash-every` ticks (default 1) and a final `complete` line. Two runs of the same build with the same arguments print identical lines; CI compares two separate processes this way (Tier 2). `save` is `run` plus `--out FILE`; the file must not exist, so a save is never silently overwritten. `load` restores a slice save, prints its tick and state hash, and optionally continues for `--ticks` more. Saves from the deleted toy world are refused.

`--script FILE` accepts blank lines, comments beginning with `#`, and these operations, one per line:

```text
step N                          # apply N explicit steps
afflict first|RAW_ID SEVERITY [DURATION_US]   # illness on a resident (default 10 s)
camera EAST NORTH RADIUS_MM     # move the camera interest
```

The integration tests run the real binary in separate processes: two runs agree byte for byte, save then load agrees on the saved hash and continues, an existing save is never overwritten, and the removed toy modes and options fail.
