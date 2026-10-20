/* An actual C consumer of the generated header and host library. */
#include "ws_ffi.h"
#include <assert.h>
#include <stddef.h>

_Static_assert(ABI_VERSION == 4, "ABI v4 required");
_Static_assert(sizeof(WsConfig) == 32, "config layout");
_Static_assert(sizeof(WsControlState) == 16, "control layout");
_Static_assert(_Alignof(WsControlState) == 4, "control alignment");
_Static_assert(offsetof(WsControlState, abi_version) == 0, "version offset");
_Static_assert(offsetof(WsControlState, struct_size) == 4, "size offset");
_Static_assert(offsetof(WsControlState, scale_tag) == 8, "scale offset");
_Static_assert(offsetof(WsControlState, reserved) == 12, "reserved offset");
_Static_assert(sizeof(WsSnapshotHeader) == 24, "snapshot layout");
_Static_assert(sizeof(WsInstance) == 32, "record layout");
_Static_assert(offsetof(WsInstance, a) == 24 && offsetof(WsInstance, b) == 28, "record a/b");

_Static_assert(sizeof(WsUtcLabel) == 40 && _Alignof(WsUtcLabel) == 4, "UTC layout");
_Static_assert(offsetof(WsUtcLabel, year) == 8 && offsetof(WsUtcLabel, reserved) == 36, "UTC fields");
_Static_assert(sizeof(WsCalendarSky) == 160 && _Alignof(WsCalendarSky) == 8, "sky layout");
_Static_assert(offsetof(WsCalendarSky, tick) == 8 && offsetof(WsCalendarSky, utc_year) == 32, "sky boundary");
_Static_assert(offsetof(WsCalendarSky, local_year) == 60 && offsetof(WsCalendarSky, utc_offset_seconds) == 88, "sky local");
_Static_assert(offsetof(WsCalendarSky, scale_tag) == 92 && offsetof(WsCalendarSky, flags) == 104, "sky versions");
_Static_assert(offsetof(WsCalendarSky, site_dataset_version) == 112 && offsetof(WsCalendarSky, sun_east) == 120, "sky site/direction");
_Static_assert(offsetof(WsCalendarSky, elevation_deg) == 152, "sky elevation");
int main(void) {
    assert(ws_abi_version() == 4);
    WsConfig config = {2, 32, 7, 0, 1, 0};
    assert(ws_create(&config) == NULL); /* Old client must be refused. */
    config.abi_version = 4;
    WsHandle *world = ws_create(&config);
    assert(world != NULL);
    const uint8_t old_batch[] = {1,0,0,0,0,0,0,0};
    assert(ws_submit(world, old_batch, sizeof old_batch) == VERSION_MISMATCH);
    const uint8_t pause[] = {2,0,0,0,1,0,0,0,1,0,0,0,12,0,0,0,1,0,0,0,0,0,0,0,0,0,0,0};
    assert(ws_submit(world, pause, sizeof pause) == OK);
    WsControlState state = {4, 16, 999, 999};
    assert(ws_get_control_state(world, &state) == OK);
    assert(state.scale_tag == 1); /* Still pending. */
    assert(ws_advance(world, 0) == OK);
    assert(ws_get_control_state(world, &state) == OK);
    assert(state.abi_version == 4 && state.struct_size == 16);
    assert(state.scale_tag == 0 && state.reserved == 0);
    assert(ws_advance(world, 1) == OK); /* Explicit tick while paused. */
    uint8_t snapshot[24 + 32 * 10]; /* 5 buildings + 5 residents */
    size_t written = 0;
    assert(ws_fill_render_snapshot(world, snapshot, sizeof snapshot, &written) == OK);
    assert(written == sizeof snapshot && snapshot[0] == 4 && snapshot[8] == 1 && snapshot[16] == 10 && snapshot[32] == 1 && snapshot[24 + 5 * 32 + 8] == 2);
    WsUtcLabel utc = {4, 40, 2026, 6, 21, 17, 0, 0, 0, 0};
    int64_t time = -1;
    assert(ws_utc_to_sim(&utc, &time) == OK);
    assert(time > 0);
    WsCalendarSky sky = {.abi_version = 4, .struct_size = 160};
    assert(ws_get_calendar_sky(world, &sky) == OK);
    assert(sky.tick == 1 && sky.scale_tag == 0 && sky.flags == 1);
    assert(sky.utc_year == 2000 && sky.site_dataset_version != 0);
    uint64_t before_hash = sky.state_hash;
    assert(ws_get_calendar_sky(world, &sky) == OK);
    assert(sky.state_hash == before_hash && sky.tick == 1);
    for (uint32_t old = 1; old <= 3; old++) {
        WsConfig old_config = config; old_config.abi_version = old;
        assert(ws_create(&old_config) == NULL);
        WsCalendarSky rejected = {.abi_version = old, .struct_size = 160, .tick = 999};
        assert(ws_get_calendar_sky(world, &rejected) == VERSION_MISMATCH);
        assert(rejected.tick == 999);
        WsControlState rejected_control = {old, 16, 999, 999};
        assert(ws_get_control_state(world, &rejected_control) == VERSION_MISMATCH);
        assert(rejected_control.scale_tag == 999);
        utc.abi_version = old; time = -1;
        assert(ws_utc_to_sim(&utc, &time) == VERSION_MISMATCH);
        assert(time == -1);
    }
    ws_destroy(world);
    return 0;
}
