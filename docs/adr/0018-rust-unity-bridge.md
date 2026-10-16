# ADR 0018: TP-003 Rust–Unity bridge contract (S-3)

- Status: Proposed
- Date: 2026-10-16
- Decided by: Pending Noah's acceptance; proposed by Codex; independent review pending
- Issue: #35 (design phase of #4)
- Supersedes: none

## Context

[TP-003](https://github.com/ndabney562/world-simulator/issues/4) must prove commands-in / snapshots-out without per-entity calls. Rust owns state; Unity owns presentation. Current `ws-sim::World` uses a 100 ms base step, rejects unaligned start times, and can fail a step without applying that step. Unity's Bridge and Rendering assemblies are skeletons; neither a native library nor bindings exist. TP-006/007 have landed, but #4 explicitly keeps debug markers until TP-011 switches to real entities. This proposal preserves that scope.

The conceptual budgeted scheduler, triple buffering, interest queries, and complete command journal are later work. This ADR specifies a synchronous prototype subset, not an implementation of those future APIs. It changes no existing accepted decision.

## Options considered

1. **Hand-written C ABI, cbindgen header, hand-written C#.** Small inspectable contract; layout and lifetime tests must keep both languages aligned. Selected for the prototype.
2. **Higher-level generated bindings.** Reduce handwritten marshalling, but add generator/runtime integration to validate with Unity IL2CPP. Revisit if the surface grows.
3. **Per-entity getters or native-to-managed callbacks.** Easy initial demo, but increases crossings and complicates ownership; conflicts with the coarse boundary.

## Proposed decision

### 1. Library and ABI

Create `ws-ffi` as the only crate Unity links (`cdylib` for macOS Editor, `staticlib` for iOS). Keep unsafe Rust confined there, with a `// SAFETY:` explanation at each block. Public exports use C calling convention and stable symbol names. The implementation pins a permissively licensed cbindgen version and commits its generated C header; CI regenerates and rejects a diff.

ABI version starts at **1**. Any incompatible function, config, status, command-envelope, or snapshot change increments it; C# checks exact equality before creating a handle. Do not silently decode a newer version. Use explicit fixed-width integers for all fields and status values, `size_t` only for native byte lengths (C# `UIntPtr`), and no Rust enum, bool, string, reference, Vec, or owned allocation across the boundary.

```c
typedef struct WsHandle WsHandle; /* opaque, Rust-owned */
typedef uint32_t WsStatus;
uint32_t ws_abi_version(void);
WsHandle *ws_create(const WsConfig *config);
void ws_destroy(WsHandle *handle);
WsStatus ws_submit(WsHandle *handle, const uint8_t *bytes, size_t len);
WsStatus ws_advance(WsHandle *handle, uint32_t steps);
WsStatus ws_fill_render_snapshot(WsHandle *handle, uint8_t *buf,
                                  size_t cap, size_t *written);
size_t ws_last_error(WsHandle *handle, char *buf, size_t cap);
```

`WsConfig` is `repr(C)` with explicit fields and offsets: `abi_version: u32` (0), `struct_size: u32` (4), `seed: u64` (8), `start_us: i64` (16), `debug_marker_count: u32` (24), `reserved: u32` (28). Size 32, alignment 8 on supported 64-bit targets. Require version 1, size 32, reserved zero, start divisible by 100,000 µs, and marker count 0..100,000; invalid configuration returns null with a diagnostic. These are debug markers, not canonical agents. Config memory is borrowed only during create; Rust owns the resulting handle. No host-clock reads in the core.

Status numbers: `OK=0`, `INVALID_ARGUMENT=1`, `VERSION_MISMATCH=2`, `BUFFER_TOO_SMALL=3`, `INVALID_COMMAND=4`, `STEP_FAILED=5`, `PANIC=6`, `POISONED=7`. Never renumber them within an ABI version. Creation reports failure as null; last-error provides the reason.

### 2. Lifetimes, errors, and threading

Each live handle is exclusively used by one caller at a time, including reads and destruction. C# serializes operations and disposal with one owner/lock; native code does not call Unity or retain caller pointers. Thread transfer between serialized calls is allowed; concurrent access to the same handle is not. Different handles are independent. A future worker must follow this ownership rule and publish only completed buffers; no worker/ring implementation is required in TP-003.

Null handles return INVALID_ARGUMENT on status-returning calls; `ws_destroy(NULL)` is a no-op. A non-null handle must come from create and be destroyed exactly once. Dangling, invented, double-destroyed, or concurrently destroyed pointers are caller contract violations, not safely detectable errors. Validate nulls, lengths and arithmetic before constructing slices, but do not claim to validate arbitrary pointer provenance. Non-null buffers must refer to live suitably sized memory, with exclusive access for writes and no alias with handle storage or out-parameters.

Wrap exports so Rust unwinding never crosses C. Use `catch_unwind` with an unwind-enabled build, not `panic=abort`; it does not recover from undefined behavior, aborts or allocation failure. An operational panic marks that handle poisoned; only last-error and destruction are allowed afterward. Do not promise rollback after a panic. Destruction must contain unwinding too; destructors must not panic. Normal step errors leave completed prior ticks applied, stop at the failing tick, and return STEP_FAILED. The subsequent snapshot exposes the reached tick; no full-batch rollback is promised.

`ws_last_error` returns the required UTF-8 byte length **including NUL**. For nonzero cap, write a valid UTF-8 prefix plus NUL, without splitting a codepoint; cap=0 permits null buf and writes nothing. A nonzero cap with null buf writes nothing and returns 0. Reading the error does not clear it. Store bounded per-handle diagnostics; failures without a usable handle, including create, use bounded thread-local diagnostics read with `ws_last_error(NULL, ...)` on the same thread. Successful operational calls clear that handle's diagnostic; panic fallback diagnostics use preallocated storage. No per-frame managed string conversion on success.

### 3. Prototype command and advance behavior

`ws_submit` borrows its bytes only for the call. The prototype wire envelope is little-endian: `u32 version=1`, `u32 count`, then `count` records of `u32 opcode`, `u32 payload_len`, followed by payload bytes. Maximum batch 64 KiB and count 1,024; empty batch uses an 8-byte header. Only opcode 0 (NoOp), length 0, is accepted. Reject unknown versions/opcodes, excessive count/size, truncated records, nonzero payloads, overflow and trailing bytes; validate the whole batch before accepting any part. Null bytes with len=0 is INVALID_COMMAND (missing envelope); null with positive len is INVALID_ARGUMENT.

NoOp has no state effect. Acceptance is transport validation only, not journal persistence. TP-008 must define semantic command ordering, application and journal outcomes in its own ADR; changes to this transport require ABI compatibility review. No LOD/time-scale mutators are exported by TP-003.

`ws_advance` runs exactly the requested number of base steps unless a step fails; zero steps succeeds. Reject requests over 100 steps as INVALID_ARGUMENT without advancing. The Unity host owns the elapsed-time accumulator and bounded catch-up policy for this prototype and calls advance only for whole steps. This does not implement the conceptual wall-clock-budget scheduler or a second time-scale authority.

### 4. Snapshot layout and buffer ownership

Snapshot v1 is little-endian bytes with a 24-byte header and 32-byte records. Rust `repr(C)` mirror structs and C# explicit layouts must match these offsets, but encode/decode bytes deliberately rather than assuming `NativeArray<byte>` is suitably aligned for typed dereferences. Initialize every byte, including reserved fields.

| Header offset | Field |
|---|---|
| 0 | `u32 abi_version` = 1 |
| 4 | `u32 header_size` = 24 |
| 8 | `u64 tick` (steps since creation) |
| 16 | `u32 instance_count` |
| 20 | `u32 record_size` = 32 |

| Record offset | Field |
|---|---|
| 0 | `u64 id` |
| 8 | `u32 kind` (0 = debug marker) |
| 12, 16, 20 | `f32 east`, `north`, `up` in metres |
| 24 | `f32 heading` in radians, from east toward north |
| 28 | `u32 reserved` = 0 |

Markers use IDs 1..N and deterministic finite transforms derived only from `(seed, tick, index)`. These IDs are debug render IDs, never canonical EntityIds; kind 0 is reserved for them. They live in a nearby synthetic ENU frame, not absolute Earth coordinates. Position conversion happens once in C# decoding: Unity `(x,y,z)=(east,up,north)` and forward direction `(cos(heading),0,sin(heading))`. Camera placement is relative to that local origin. Real-world frame/origin metadata and entity-kind mapping must be designed for TP-011 before switch-over; v1 is not silently repurposed as a geospatial snapshot.

`written` is required, non-null, aligned, and disjoint from buf. For a valid handle set it to zero first. Checked required size is `24 + count * 32`. If cap is too small, return BUFFER_TOO_SMALL, set written to required size, and leave buf untouched. A null buf with cap=0 is a supported capacity probe; if cap is sufficient but buf is null return INVALID_ARGUMENT. On success write exactly required bytes and set written accordingly; unused capacity is untouched. Filling changes no authoritative state and returns a coherent single-tick snapshot. On errors other than BUFFER_TOO_SMALL, written remains zero when it can be safely set. If written itself is null, return INVALID_ARGUMENT without touching buf.

Unity owns one persistent `NativeArray<byte>` for the synchronous demo; Rust never allocates, frees or retains it. Preallocate `24 + N*32` bytes from config outside the frame loop. Normal frames perform **one** fill call; probe/resize only during initialization or after a capacity error, outside the normal frame path. Never render a partial/error buffer. Dispose only after all native calls using it finish. Decoder checks lengths with overflow-safe arithmetic, ABI/header/record sizes, finite transforms, reserved fields and supported kinds before publishing data. Reuse rendering arrays; measure allocations in the Profiler.

### 5. C# and packaging

Use a SafeHandle owner for the native handle, matching DllImport Cdecl declarations and fixed-width layouts. Create manually rejects a null return; calls keep the SafeHandle alive, and disposal is serialized with all uses. Explicit disposal on scene teardown/domain reload is required; finalization is fallback cleanup, with native destruction independent of Unity APIs. On iOS device builds import `__Internal`; in the macOS Editor import `ws_ffi`. Other platforms fail clearly until supported, rather than simulating success.

Passing the persistent NativeArray pointer requires a small unsafe C# boundary in Bridge (its current asmdef has unsafe disabled). The implementation may enable unsafe there, confining pointer acquisition to one reviewed adapter; Rendering remains safe. No pointer escapes a synchronized native call. Confirm required Unity package references during implementation, without adding unnecessary packages.

`scripts/build-native-macos.sh` builds a release dylib for the running Editor architecture into ignored `Assets/Plugins/macOS/`; detect unsupported architecture rather than mixing arm64 and x86-64. Native binaries are not committed. A fresh library requires an Editor restart to avoid testing a stale loaded image. CI builds `aarch64-apple-ios` on macOS, checks generated-header freshness, and tests layouts/error paths. Physical device linking/signing and profiling stay TP-011 and human-gated.

### 6. Acceptance evidence for implementation

- Rust tests exercise every export; invalid config/alignment/version/count, null handles, zero steps, step failure after earlier successful steps, capacity probe, undersized/exact/oversized buffers, canaries, malformed NoOp batches, poison behavior, and error truncation. Do not test invalid dangling pointer calls as if they were supported.
- Layout assertions cover every offset, size and alignment; use a fixed snapshot byte fixture decoded by both Rust and C#.
- macOS EditMode test loads the actual dylib, rejects ABI mismatch, creates/advances/fills/decodes expected marker count and tick, verifies coordinate basis/heading, disposes resources, and proves fill/NoOp leave the state hash unchanged.
- Bootstrap visibly draws moving instanced markers exclusively from snapshots. Attach Editor evidence; measure 1,000/10,000 marker fill times with Criterion and managed allocations with Profiler. Device ≤2 ms fill target is measured later, not claimed from desktop results.
- CI passes existing checks plus iOS-target build and header freshness; demonstrate that a stale generated header fails. Run tests on current macOS Editor and document IL2CPP/device verification still outstanding.

## Consequences and revisit triggers

The ABI is small and explicit; most safety depends on the thin binding honoring pointer, disposal and serialization contracts. C# layout/fixture tests and native header checks are mandatory maintenance. Error handling does not make untrusted pointers safe. Debug marker counts and fixed-step caps bound prototype work without claiming a frame budget.

No implementation, dependency addition, accepted-ADR change, signing or provider work is authorized by this Proposed document alone. After Noah accepts it, #4 supplies the implementation scope. Revisit before real entity snapshots, larger areas, asynchronous workers/triple buffering, semantic commands, or additional platforms. These changes need an explicit ABI/version review.

## Technical references

- [Rust FFI guidance](https://doc.rust-lang.org/nomicon/ffi.html): C calling conventions, pointer validity and unwind boundaries.
- [.NET native interop guidance](https://learn.microsoft.com/en-us/dotnet/standard/native-interop/best-practices): managed binding and resource ownership considerations. Unity Mono/IL2CPP behavior still requires project tests.
- Existing project constraints: [ADR 0014](0014-time-representation.md), [ADR 0015](0015-coordinate-representation.md), [rendering architecture](../architecture/03-rendering-architecture.md), and [simulation architecture](../architecture/04-simulation-architecture.md).
