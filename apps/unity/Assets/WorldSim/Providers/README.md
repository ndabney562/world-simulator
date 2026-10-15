# Providers

Placeholder. Visual backdrop providers (open-data tiles, Google Photorealistic 3D Tiles, NASA imagery), the provider manager, quota tracking, and attribution will live here, and only here (`docs/architecture/17-repository-structure.md` §18.2 rule 4).

Providers draw the backdrop and never modify simulation state (ADR 0008). They do not gate the core Rust → FFI → Unity → iPhone path (ADR 0013).

TP-002 adds no code to this folder; provider work is TP-012. The edit-mode test `ProvidersFolderContainsNoCode` fails if C# appears here before then.
