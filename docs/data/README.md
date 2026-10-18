# Data-source register

Every external dataset the project uses is registered here with its licence, attribution text, and version (decision D-22, [21-decisions-now-vs-later.md](../architecture/21-decisions-now-vs-later.md); rule 7 of [17-repository-structure.md](../architecture/17-repository-structure.md) §18.2). Free, openly licensed sources only ([ADR 0007](../adr/0007-zero-cost-policy.md)). Google data is never simulation input ([ADR 0008](../adr/0008-visual-backdrop-providers.md)).

| Source | Publisher | Licence | Version used | Used by |
|---|---|---|---|---|
| [NYC Building Footprints](nyc-building-footprints.md) | NYC Office of Technology and Innovation | NYC Open Data, no restrictions on use | Snapshot retrieved 2026-10-18 (dataset updated 2026-10-11) | [Block 6049 extract](../../data/vertical-slice/bay-ridge-block-6049/README.md) |
| [NYC PLUTO](nyc-pluto.md) | NYC Department of City Planning | NYC Open Data, no restrictions on use | 26v2, retrieved 2026-10-18 | [Block 6049 extract](../../data/vertical-slice/bay-ridge-block-6049/README.md) |

## Adding a source

Add one page per source with: publisher, dataset URL and ID, licence and the text it requires, attribution text to show in the app's credits, version or retrieval date, the fields used and their units, known accuracy limits, and which extracts use it. Add a row above. A source whose licence is unclear is not used until the licence is confirmed.
