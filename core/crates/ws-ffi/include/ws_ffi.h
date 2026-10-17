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
#define ABI_VERSION 3

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
 * Opaque C handle. Never construct, dereference or copy its storage in a caller.
 */
typedef struct WsHandle WsHandle;

/**
 * Creation configuration, 32 bytes aligned to 8 on supported targets.
 */
typedef struct {
  /**
   * Must equal 3.
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
   * Caller requests ABI 3; returned as 3 on success.
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
 * Snapshot v3 header, serialized little-endian without alignment assumptions.
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

#endif  /* WS_FFI_H */
