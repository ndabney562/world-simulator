#ifndef WS_FFI_H
#define WS_FFI_H

/* Generated with cbindgen 0.29.2; do not edit by hand. */

#include <stdarg.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

/**
 * Exact native/managed contract version.
 */
#define ABI_VERSION 9

/**
 * Most residents per building a config may ask for.
 */
#define MAX_RESIDENTS_PER_BUILDING 2000

/**
 * Successful operation.
 */
#define OK 0

/**
 * Null pointer, invalid configuration or excessive step request.
 */
#define INVALID_ARGUMENT 1

/**
 * Incompatible ABI or transport version.
 */
#define VERSION_MISMATCH 2

/**
 * Caller must provide the required output capacity.
 */
#define BUFFER_TOO_SMALL 3

/**
 * Malformed or unsupported command batch.
 */
#define INVALID_COMMAND 4

/**
 * Boundary processing or a tick failed; earlier committed work remains applied.
 */
#define STEP_FAILED 5

/**
 * Caught native unwind; handle is poisoned.
 */
#define PANIC 6

/**
 * Only diagnostics and destruction are allowed on this handle.
 */
#define POISONED 7

/**
 * A background save operation is still in progress.
 */
#define SAVE_IN_PROGRESS 8

/**
 * Opaque C handle. Never construct, dereference or copy its storage in a caller.
 */
typedef struct WsHandle WsHandle;

/**
 * Opaque serialized real-slice owner. Never cast to/from the toy WsHandle.
 */
typedef struct WsSliceHandle WsSliceHandle;

/**
 * Creation configuration, 32 bytes aligned to 8 on supported targets.
 */
typedef struct {
  /**
   * Must equal 5.
   */
  uint32_t abi_version;
  /**
   * Must equal 32.
   */
  uint32_t struct_size;
  /**
   * Root seed.
   */
  uint64_t seed;
  /**
   * Epoch-relative microseconds, divisible by 100,000.
   */
  int64_t start_us;
  /**
   * Residents in every building, 1 through 2,000 (the toy world has five buildings).
   */
  uint32_t residents_per_building;
  /**
   * Must be zero.
   */
  uint32_t reserved;
} WsConfig;

/**
 * Fixed-width C status code; numeric values retain their ABI-v1 meanings.
 */
typedef uint32_t WsStatus;

/**
 * Coarse committed control state; 16 bytes, aligned to 4. Input: version and size only.
 */
typedef struct {
  /**
   * Caller requests ABI5; returned as5 on success.
   */
  uint32_t abi_version;
  /**
   * Caller provides 16 bytes; returned as 16 on success.
   */
  uint32_t struct_size;
  /**
   * Authoritative committed scale: 0, 1, 10 or 100.
   */
  uint32_t scale_tag;
  /**
   * Returned as zero.
   */
  uint32_t reserved;
} WsControlState;

/**
 * Real-slice creation,32 bytes aligned8. Population is the pinned synthetic cohort.
 */
typedef struct {
  /**
   * Exact current native ABI9.
   */
  uint32_t abi_version;
  /**
   * Exactly32.
   */
  uint32_t struct_size;
  /**
   * Procedural seed, retaining the existing seed-generated10–30 residents.
   */
  uint64_t seed;
  /**
   * Configured100-ms-grid SimTime; creation must be in UTC2000–2099.
   */
  int64_t start_us;
  /**
   * Zero.
   */
  uint64_t reserved;
} WsSliceConfig;

/**
 * One coarse input,64 bytes aligned8; including explicit single-resident god interventions.
 */
typedef struct {
  /**
   * Exact current ABI9.
   */
  uint32_t abi_version;
  /**
   * Exactly64.
   */
  uint32_t struct_size;
  /**
   * 1steps,2scale,3host elapsed microseconds,4camera target/radius,5illness,
   * 6bless,7storm,8gathering (ADR0038).
   */
  uint32_t tag;
  /**
   * Zero.
   */
  uint32_t reserved;
  /**
   * Tags1–3: unsigned word0 and remaining words0. Tag4: four finite f64
   * bit patterns (ENU target metres,radius metres), last two words0.
   * Tag5: resident,severity(u32),duration_us. Tag6: resident. Tag7:
   * duration_us. Tag8: building,duration_us. Every unused word is zero.
   */
  uint64_t words[6];
} WsSliceOperation;

/**
 * Coherent environment output, exactly160 bytes aligned to8. Initialize only
 * abi_version/struct_size before the call; all fields are written on success.
 */
typedef struct {
  /**
   * ABI6.
   */
  uint32_t abi_version;
  /**
   * Exactly160.
   */
  uint32_t struct_size;
  /**
   * Committed physical tick; pair with the render snapshot under one gate.
   */
  uint64_t tick;
  /**
   * Committed uniform-SI microseconds from the simulation epoch.
   */
  int64_t sim_time_us;
  /**
   * Canonical state hash; querying never changes it.
   */
  uint64_t state_hash;
  /**
   * Exact UTC astronomical Gregorian year.
   */
  int32_t utc_year;
  /**
   * UTC month.
   */
  uint32_t utc_month;
  /**
   * UTC day.
   */
  uint32_t utc_day;
  /**
   * UTC hour.
   */
  uint32_t utc_hour;
  /**
   * UTC minute.
   */
  uint32_t utc_minute;
  /**
   * UTC second, including60.
   */
  uint32_t utc_second;
  /**
   * UTC microsecond.
   */
  uint32_t utc_microsecond;
  /**
   * New York Gregorian year; zero if unavailable.
   */
  int32_t local_year;
  /**
   * New York month; zero if unavailable.
   */
  uint32_t local_month;
  /**
   * New York day; zero if unavailable.
   */
  uint32_t local_day;
  /**
   * New York hour; zero if unavailable.
   */
  uint32_t local_hour;
  /**
   * New York minute; zero if unavailable.
   */
  uint32_t local_minute;
  /**
   * New York second, preserving60; zero if unavailable.
   */
  uint32_t local_second;
  /**
   * New York microsecond; zero if unavailable.
   */
  uint32_t local_microsecond;
  /**
   * EST−18000/EDT−14400, or zero if unavailable.
   */
  int32_t utc_offset_seconds;
  /**
   * Committed scale0/1/10/100, never pending commands.
   */
  uint32_t scale_tag;
  /**
   * Pinned solar rule version.
   */
  uint32_t solar_version;
  /**
   * Pinned New York rule version.
   */
  uint32_t ny_policy_version;
  /**
   * Bit0=supported local/solar; all other bits zero.
   */
  uint32_t flags;
  /**
   * Embedded UTC leap table version.
   */
  uint32_t leap_table_version;
  /**
   * Registered block dataset providing the solar site.
   */
  uint64_t site_dataset_version;
  /**
   * Toward-sun east in the paired render ENU basis; zero if unavailable.
   */
  double sun_east;
  /**
   * Toward-sun north in the paired render ENU basis; zero if unavailable.
   */
  double sun_north;
  /**
   * Toward-sun up in the paired render ENU basis; zero if unavailable.
   */
  double sun_up;
  /**
   * Geometric azimuth clockwiseNorth at the fixed site; zero if unavailable.
   */
  double azimuth_deg;
  /**
   * Geometric elevation at the fixed site; zero if unavailable.
   */
  double elevation_deg;
} WsCalendarSky;

/**
 * Validated UTC input label, exactly40 bytes aligned to4.
 */
typedef struct {
  /**
   * ABI6.
   */
  uint32_t abi_version;
  /**
   * Exactly40.
   */
  uint32_t struct_size;
  /**
   * Astronomical Gregorian year; conversion separately checks SimTime range.
   */
  int32_t year;
  /**
   * Month1..=12.
   */
  uint32_t month;
  /**
   * Day of month.
   */
  uint32_t day;
  /**
   * Hour0..=23.
   */
  uint32_t hour;
  /**
   * Minute0..=59.
   */
  uint32_t minute;
  /**
   * Second0..=60,60 only at a tabulated UTC leap second.
   */
  uint32_t second;
  /**
   * Microsecond0..=999999.
   */
  uint32_t microsecond;
  /**
   * Must be zero.
   */
  uint32_t reserved;
} WsUtcLabel;

/**
 * Snapshot v5 header, serialized little-endian without alignment assumptions.
 */
typedef struct {
  /**
   * ABI version.
   */
  uint32_t abi_version;
  /**
   * Header bytes (24).
   */
  uint32_t header_size;
  /**
   * Completed base steps.
   */
  uint64_t tick;
  /**
   * Instance count.
   */
  uint32_t instance_count;
  /**
   * Instance record bytes (32).
   */
  uint32_t record_size;
} WsSnapshotHeader;

/**
 * Render record (ADR 0022): kind 1 is a building box, kind 2 a resident capsule.
 */
typedef struct {
  /**
   * Raw entity ID.
   */
  uint64_t id;
  /**
   * 1 building, 2 resident.
   */
  uint32_t kind;
  /**
   * East metres of the snapshot origin.
   */
  float east;
  /**
   * North metres of the snapshot origin.
   */
  float north;
  /**
   * Up metres of the snapshot origin.
   */
  float up;
  /**
   * Building half extent in metres, or resident heading in radians.
   */
  float a;
  /**
   * Building height in metres, or zero.
   */
  float b;
} WsInstance;

/**
 * Returns ABI version without creating a world.
 */
uint32_t ws_abi_version(void);

/**
 * Creates a world; null on invalid config or caught panic. Read last_error(NULL).
 * # Safety
 * Non-null config must point to one aligned, initialized WsConfig for this call.
 */
WsHandle *ws_create(const WsConfig *config);

/**
 * Destroys exactly one live handle; null is a no-op.
 * # Safety
 * Non-null handle must be from create, live, exclusively owned and not destroyed before.
 */
void ws_destroy(WsHandle *handle);

/**
 * Atomically copies a bounded v2 batch into the journaled queue; success means pending.
 * # Safety
 * Handle must be live/exclusive; positive len needs readable bytes for the call.
 */
WsStatus ws_submit(WsHandle *handle, const uint8_t *bytes, size_t len);

/**
 * Flushes once, then advances explicit whole steps (0..100), independent of scale.
 * Zero steps flush even while paused. Invalid step counts fail before flushing.
 * # Safety
 * Handle must be live and exclusively borrowed for this call.
 */
WsStatus ws_advance(WsHandle *handle, uint32_t steps);

/**
 * Reads committed control state without flushing, stepping or publishing events.
 * Errors leave the destination unchanged and use the handle diagnostic when usable.
 * # Safety
 * Handle must be live/exclusive. A non-null out_state must be aligned writable
 * storage for 16 bytes, disjoint from handle; its first two u32 fields must be initialized.
 */
WsStatus ws_get_control_state(WsHandle *handle, WsControlState *out_state);

/**
 * Fills a coherent snapshot; undersized buffers are untouched and written gives required bytes.
 * # Safety
 * Handle must be live/exclusive. Written must be aligned writable usize storage, disjoint
 * from buf/handle; non-null buf must be writable for cap bytes without other access.
 */
WsStatus ws_fill_render_snapshot(WsHandle *handle, uint8_t *buf, size_t cap, size_t *written);

/**
 * Copies bounded UTF-8 diagnostic; required length includes NUL. Null handle reads thread-local error.
 * # Safety
 * Non-null handle must be live/exclusive; non-null buf must be writable for cap bytes.
 */
size_t ws_last_error(WsHandle *handle,
                     char *buf,
                     size_t cap);

/**
 * Creates an explicit real slice, null on validation/construction failure or panic.
 * Creation uses the stored calendar-aligned Monday epoch; no wall clock is read.
 * # Safety
 * Non-null config points to one readable aligned initialized WsSliceConfig.
 */
WsSliceHandle *ws_slice_create(const WsSliceConfig *config);

/**
 * Destroys exactly one live slice handle; null is a no-op.
 * # Safety
 * Non-null handle is live, exclusively owned and not destroyed before.
 */
void ws_slice_destroy(WsSliceHandle *handle);

/**
 * Validates then applies one coarse operation. Failure preserves the core's
 * committed prefix and recorded stop; query the snapshot for that boundary.
 * Wire validation failure performs no domain operation. No pointers are retained.
 * # Safety
 * Handle is live/exclusive; input points to one readable aligned initialized operation.
 */
WsStatus ws_slice_apply(WsSliceHandle *handle, const WsSliceOperation *input);

/**
 * Copies the once-built static geometry. Short capacity reports required bytes
 * without touching data; other failures leave both output arguments unchanged.
 * # Safety
 * Handle is live/exclusive; non-null buffer is writable for capacity bytes;
 * written is aligned writable usize storage, disjoint from buffer/handle.
 */
WsStatus ws_slice_fill_geometry(WsSliceHandle *handle,
                                uint8_t *buffer,
                                size_t capacity,
                                size_t *written);

/**
 * Copies one pure complete committed resident/count snapshot. All serialization
 * finishes in bounded scratch storage before any caller data is written.
 * # Safety
 * Same live/exclusive/disjoint buffer contract as ws_slice_fill_geometry.
 */
WsStatus ws_slice_fill_snapshot(WsSliceHandle *handle,
                                uint8_t *buffer,
                                size_t capacity,
                                size_t *written);

/**
 * Pure160-byte slice sky query in the same fixed geometry basis. Caller initializes
 * only ABI/size; no output bytes change on validation error, poison or panic.
 * # Safety
 * Handle is live/exclusive; output is aligned writable160-byte storage disjoint
 * from the handle. Serialize across dynamic fill and this query before pairing.
 */
WsStatus ws_slice_get_calendar_sky(WsSliceHandle *handle, WsCalendarSky *output);

/**
 * Saves the real slice synchronously to a format-5 save file.
 * # Safety
 * Non-null handle is live/exclusive. Non-null path is null-terminated UTF-8.
 */
WsStatus ws_slice_save(WsSliceHandle *handle, const char *path_utf8);

/**
 * Spawns a background thread saving the real slice to a format-5 save file.
 * # Safety
 * Non-null handle is live/exclusive. Non-null path is null-terminated UTF-8.
 */
WsStatus ws_slice_save_background(WsSliceHandle *handle, const char *path_utf8);

/**
 * Queries the status of the current or most recent save operation.
 * Returns OK (0) if idle/completed, SAVE_IN_PROGRESS (8) if running,
 * or an error status on failure.
 * # Safety
 * Non-null handle is live/exclusive.
 */
WsStatus ws_slice_save_status(WsSliceHandle *handle);

/**
 * Loads a real slice from a format-4 or format-5 save file. Returns null on failure.
 * # Safety
 * Non-null path is null-terminated UTF-8.
 */
WsSliceHandle *ws_slice_load(const char *path_utf8);

/**
 * Copies the bounded diagnostic, including on a poisoned handle. Null reads TLS.
 * # Safety
 * Non-null handle is live/exclusive; positive capacity requires a writable buffer.
 */
size_t ws_slice_last_error(WsSliceHandle *handle, char *buffer, size_t capacity);

/**
 * One pure bounded entity inspection, schema3. Short output reports its exact
 * required length; other failures zero written and leave data untouched.
 * # Safety
 * Handle is live/exclusive; buffer is writable for capacity bytes; written is
 * aligned writable usize storage. All outputs are disjoint from each other,
 * the handle and input. No pointer is retained.
 */
WsStatus ws_slice_inspect(WsSliceHandle *handle,
                          uint64_t entity_id,
                          uint8_t *buffer,
                          size_t capacity,
                          size_t *written);

/**
 * Pure bounded real-slice produced consequences with explicit causal confidence.
 * # Safety
 * Live exclusive handle; buffer and aligned written storage writable/disjoint.
 */
WsStatus ws_slice_consequences(WsSliceHandle *handle,
                               uint64_t root,
                               uint8_t *buffer,
                               size_t capacity,
                               size_t *written);

/**
 * Pure UTC configuration conversion; no world, clock read or grid rounding.
 * Only success writes output. World creation separately enforces100-ms grid.
 * Errors and caught unwinds use thread-local diagnostics; no handle is poisoned.
 * # Safety
 * Non-null input points to one initialized aligned40-byte label; non-null
 * output points to writable aligned i64 storage, disjoint from input.
 */
WsStatus ws_utc_to_sim(const WsUtcLabel *input, int64_t *out_time);

/**
 * Pure committed-boundary calendar/sky query. Does not flush pending commands.
 * Invalid input/version/size, poison and caught panic leave output untouched.
 * # Safety
 * Handle must be live/exclusive; non-null output is aligned writable160-byte
 * storage disjoint from the handle, with only its first two u32 fields required
 * initialized. Raw callers serialize this query with render fill and mutation.
 */
WsStatus ws_get_calendar_sky(WsHandle *handle, WsCalendarSky *out_sky);

#endif  /* WS_FFI_H */
