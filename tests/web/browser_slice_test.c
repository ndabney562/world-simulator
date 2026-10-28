/*
 * Browser compatibility spike test harness for Issue #106.
 * Exercises ws-ffi on WebAssembly (wasm32-unknown-emscripten) executing the real
 * 30-resident block slice simulation, calendar sky, illness intervention,
 * causal consequences query, inspection, and format-4 SQLite save/load.
 */

#include <assert.h>
#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include <time.h>
#include "ws_ffi.h"

#define SEED 71449ULL
#define START_US 1072915200000000LL /* 2004-01-01T00:00:00Z */

static double now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (double)ts.tv_sec * 1000.0 + (double)ts.tv_nsec / 1000000.0;
}

int main(int argc, char **argv) {
    printf("=== World Simulator: Browser WebAssembly Compatibility Spike ===\n");
    printf("ABI version: %u\n", ws_abi_version());
    assert(ws_abi_version() == 8);

    /* 1. Create slice */
    printf("\n[1/7] Creating real slice (seed=%llu, start=%lld us)...\n", SEED, START_US);
    double t0 = now_ms();
    WsSliceConfig config = {
        .abi_version = 8,
        .struct_size = 32,
        .seed = SEED,
        .start_us = START_US,
        .reserved = 0
    };
    WsSliceHandle *slice = ws_slice_create(&config);
    if (!slice) {
        char err[512] = {0};
        ws_slice_last_error(NULL, err, sizeof err);
        fprintf(stderr, "Failed to create slice: %s\n", err);
        return 1;
    }
    double create_ms = now_ms() - t0;
    printf("  -> Slice created in %.2f ms\n", create_ms);

    /* 2. Static geometry */
    printf("\n[2/7] Extracting static geometry...\n");
    uint8_t static_geo[131072];
    size_t geo_written = 0;
    assert(ws_slice_fill_geometry(slice, static_geo, sizeof static_geo, &geo_written) == OK);
    assert(geo_written == 71532);
    uint32_t bldg_count = *(uint32_t *)(static_geo + 16);
    printf("  -> Static geometry: %zu bytes, %u building instances\n", geo_written, bldg_count);

    /* 3. Render snapshot & 30 residents verification */
    printf("\n[3/7] Reading render snapshot...\n");
    uint8_t snapshot[4096];
    size_t snap_written = 0;
    assert(ws_slice_fill_snapshot(slice, snapshot, sizeof snapshot, &snap_written) == OK);
    assert(snap_written == 1760);
    uint32_t resident_count = snapshot[48];
    uint32_t building_count = snapshot[56];
    printf("  -> Render snapshot: %zu bytes, %u residents, %u featured buildings\n",
           snap_written, resident_count, building_count);
    assert(resident_count == 30);

    /* Pick first resident */
    uint64_t target_resident = 0;
    for (unsigned i = 0; i < 8; ++i) {
        target_resident |= (uint64_t)snapshot[128 + i] << (8 * i);
    }
    printf("  -> Target resident ID: 0x%016llx\n", target_resident);

    /* 4. Inspection */
    printf("\n[4/7] Inspecting resident 0x%016llx before intervention...\n", target_resident);
    uint8_t inspection[8192];
    size_t insp_written = 0;
    assert(ws_slice_inspect(slice, target_resident, inspection, sizeof inspection, &insp_written) == OK);
    assert(insp_written > 96);
    unsigned identity = 0;
    for (unsigned i = 0; i < 4; ++i) identity |= (unsigned)inspection[64 + i] << (8 * i);
    unsigned state_offset = 96 + identity;
    uint8_t vitality_before = inspection[state_offset + 56];
    uint8_t health_before = inspection[state_offset + 84];
    printf("  -> Vitality: %u%%, Health state: %u (healthy)\n", vitality_before, health_before);
    assert(health_before == 0); /* 0 = healthy */

    /* 5. Speed controls, pause, and simulation advance */
    printf("\n[5/7] Testing pacing controls and simulation advance...\n");
    WsSliceOperation op = { .abi_version = 8, .struct_size = 64 };

    /* Normal speed: scale = 1 */
    op.tag = 2; op.words[0] = 1;
    assert(ws_slice_apply(slice, &op) == OK);

    /* Advance 10 steps */
    t0 = now_ms();
    op.tag = 1; op.words[0] = 10;
    assert(ws_slice_apply(slice, &op) == OK);
    double step_ms = now_ms() - t0;
    printf("  -> Advanced 10 ticks at scale 1 in %.2f ms (%.3f ms/tick)\n", step_ms, step_ms / 10.0);

    /* Calendar sky check */
    WsCalendarSky sky = { .abi_version = 8, .struct_size = 160 };
    assert(ws_slice_get_calendar_sky(slice, &sky) == OK);
    printf("  -> Sim time: tick=%llu, UTC %04d-%02u-%02u %02u:%02u:%02u\n",
           sky.tick, sky.utc_year, sky.utc_month, sky.utc_day, sky.utc_hour, sky.utc_minute, sky.utc_second);
    printf("  -> Solar position: elevation=%.2f deg, azimuth=%.2f deg\n", sky.elevation_deg, sky.azimuth_deg);
    assert(sky.tick == 10);

    /* Pause: scale = 0 */
    op.tag = 2; op.words[0] = 0;
    assert(ws_slice_apply(slice, &op) == OK);
    assert(ws_slice_get_calendar_sky(slice, &sky) == OK && sky.scale_tag == 0);
    printf("  -> Paused simulator (scale_tag=0)\n");

    /* 6. Illness intervention and consequence chain */
    printf("\n[6/7] Applying illness intervention (tag 5)...\n");
    op.tag = 5;
    op.words[0] = target_resident;
    op.words[1] = 5;       /* symptom severity */
    op.words[2] = 200000;  /* duration */
    op.words[3] = 0; op.words[4] = 0; op.words[5] = 0;
    assert(ws_slice_apply(slice, &op) == OK);
    printf("  -> Inoculated resident 0x%016llx with acute illness\n", target_resident);

    /* Inspect resident immediately */
    assert(ws_slice_inspect(slice, target_resident, inspection, sizeof inspection, &insp_written) == OK);
    uint8_t vitality_ill = inspection[state_offset + 56];
    uint8_t health_ill = inspection[state_offset + 84];
    uint64_t causal_root = 0;
    for (unsigned i = 0; i < 8; ++i) causal_root |= (uint64_t)inspection[state_offset + 96 + i] << (8 * i);
    printf("  -> Post-infection Vitality: %u%% (was %u%%), Health state: %u (acute), Root: 0x%016llx\n",
           vitality_ill, vitality_before, health_ill, causal_root);
    assert(health_ill == 1); /* 1 = afflicted */
    assert(vitality_ill < vitality_before);

    /* Query consequences */
    uint8_t consequences[8192];
    size_t cons_written = 0;
    assert(ws_slice_consequences(slice, causal_root, consequences, sizeof consequences, &cons_written) == OK);
    printf("  -> Query consequences: %zu bytes written for causal chain 0x%016llx\n", cons_written, causal_root);
    assert(cons_written > 96);

    /* Advance simulation: fast speed scale 10 */
    memset(op.words, 0, sizeof op.words);
    op.tag = 2; op.words[0] = 10;
    assert(ws_slice_apply(slice, &op) == OK);
    memset(op.words, 0, sizeof op.words);
    op.tag = 1; op.words[0] = 2;
    assert(ws_slice_apply(slice, &op) == OK);

    /* Verify recovery */
    assert(ws_slice_inspect(slice, target_resident, inspection, sizeof inspection, &insp_written) == OK);
    uint8_t vitality_recovered = inspection[state_offset + 56];
    uint8_t health_recovered = inspection[state_offset + 84];
    printf("  -> Advanced simulation: Vitality: %u%%, Health state: %u (recovered)\n",
           vitality_recovered, health_recovered);
    assert(health_recovered == 0);

    /* 7. Save and Load SQLite database in WebAssembly filesystem */
    printf("\n[7/7] SQLite persistence in WebAssembly virtual filesystem...\n");
    /* Actual browser hosts submit irregular elapsed intervals, not just Steps. */
    memset(op.words, 0, sizeof op.words);
    op.tag = 3; op.words[0] = 50003;
    assert(ws_slice_apply(slice, &op) == OK);
    op.words[0] = 53127;
    assert(ws_slice_apply(slice, &op) == OK);
    assert(ws_slice_get_calendar_sky(slice, &sky) == OK);
    const char *save_path = "/tmp/browser_slice_test.wssave";
    t0 = now_ms();
    assert(ws_slice_save(slice, save_path) == OK);
    double save_ms = now_ms() - t0;
    printf("  -> Synchronous SQLite save completed in %.2f ms to %s\n", save_ms, save_path);
    assert(ws_slice_save_status(slice) == OK);

    /* Load saved slice */
    t0 = now_ms();
    WsSliceHandle *loaded = ws_slice_load(save_path);
    double load_ms = now_ms() - t0;
    if (!loaded) {
        char err[512] = {0};
        ws_slice_last_error(NULL, err, sizeof err);
        fprintf(stderr, "Failed to load saved slice: %s\n", err);
        return 1;
    }
    printf("  -> SQLite load and state-hash verification completed in %.2f ms\n", load_ms);

    WsCalendarSky loaded_sky = { .abi_version = 8, .struct_size = 160 };
    assert(ws_slice_get_calendar_sky(loaded, &loaded_sky) == OK);
    assert(ws_slice_get_calendar_sky(slice, &sky) == OK);
    printf("  -> State hashes: original=0x%016llx, loaded=0x%016llx (agree: %s)\n",
           sky.state_hash, loaded_sky.state_hash, sky.state_hash == loaded_sky.state_hash ? "YES" : "NO");
    assert(sky.state_hash == loaded_sky.state_hash);
    assert(sky.tick == loaded_sky.tick);

    ws_slice_destroy(loaded);
    ws_slice_destroy(slice);

    printf("\n=== SPIKE RESULT: ALL 7 WEBASSEMBLY CONTRACT VERIFICATIONS PASSED ===\n");
    return 0;
}
