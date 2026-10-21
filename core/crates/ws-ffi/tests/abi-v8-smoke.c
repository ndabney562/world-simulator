/* An actual C consumer of the generated header and host library. */
#include "ws_ffi.h"
#include <assert.h>
#include <stddef.h>

_Static_assert(ABI_VERSION == 8, "ABI v8 required");
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
_Static_assert(sizeof(WsSliceConfig) == 32 && _Alignof(WsSliceConfig) == 8, "slice config layout");
_Static_assert(offsetof(WsSliceConfig, seed) == 8 && offsetof(WsSliceConfig, start_us) == 16 && offsetof(WsSliceConfig, reserved) == 24, "slice config fields");
_Static_assert(sizeof(WsSliceOperation) == 64 && _Alignof(WsSliceOperation) == 8, "slice operation layout");
_Static_assert(offsetof(WsSliceOperation, tag) == 8 && offsetof(WsSliceOperation, words) == 16, "slice operation fields");
int main(void) {
    assert(ws_abi_version() == 8);
    WsConfig config = {2, 32, 7, 0, 1, 0};
    assert(ws_create(&config) == NULL); /* Old client must be refused. */
    config.abi_version = 8;
    WsHandle *world = ws_create(&config);
    assert(world != NULL);
    const uint8_t old_batch[] = {1,0,0,0,0,0,0,0};
    assert(ws_submit(world, old_batch, sizeof old_batch) == VERSION_MISMATCH);
    const uint8_t pause[] = {2,0,0,0,1,0,0,0,1,0,0,0,12,0,0,0,1,0,0,0,0,0,0,0,0,0,0,0};
    assert(ws_submit(world, pause, sizeof pause) == OK);
    WsControlState state = {8, 16, 999, 999};
    assert(ws_get_control_state(world, &state) == OK);
    assert(state.scale_tag == 1); /* Still pending. */
    assert(ws_advance(world, 0) == OK);
    assert(ws_get_control_state(world, &state) == OK);
    assert(state.abi_version == 8 && state.struct_size == 16);
    assert(state.scale_tag == 0 && state.reserved == 0);
    assert(ws_advance(world, 1) == OK); /* Explicit tick while paused. */
    uint8_t snapshot[24 + 32 * 10]; /* 5 buildings + 5 residents */
    size_t written = 0;
    assert(ws_fill_render_snapshot(world, snapshot, sizeof snapshot, &written) == OK);
    assert(written == sizeof snapshot && snapshot[0] == 8 && snapshot[8] == 1 && snapshot[16] == 10 && snapshot[32] == 1 && snapshot[24 + 5 * 32 + 8] == 2);
    WsUtcLabel utc = {8, 40, 2026, 6, 21, 17, 0, 0, 0, 0};
    int64_t time = -1;
    assert(ws_utc_to_sim(&utc, &time) == OK);
    assert(time > 0);
    WsCalendarSky sky = {.abi_version = 8, .struct_size = 160};
    assert(ws_get_calendar_sky(world, &sky) == OK);
    assert(sky.tick == 1 && sky.scale_tag == 0 && sky.flags == 1);
    assert(sky.utc_year == 2000 && sky.site_dataset_version != 0);
    uint64_t before_hash = sky.state_hash;
    assert(ws_get_calendar_sky(world, &sky) == OK);
    assert(sky.state_hash == before_hash && sky.tick == 1);
    for (uint32_t old = 1; old <= 7; old++) {
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

    utc.abi_version = 8;
    assert(ws_utc_to_sim(&utc, &time) == OK);
    WsSliceConfig slice_config = {8, 32, 71449, time, 0};
    for (uint32_t old = 1; old <= 7; old++) {
        slice_config.abi_version = old;
        assert(ws_slice_create(&slice_config) == NULL);
    }
    slice_config.abi_version = 8;
    WsSliceHandle *slice = ws_slice_create(&slice_config);
    assert(slice != NULL);
    written = 0;
    assert(ws_slice_fill_geometry(slice, NULL, 0, &written) == BUFFER_TOO_SMALL);
    assert(written == 71532);
    uint8_t geometry[71532];
    assert(ws_slice_fill_geometry(slice, geometry, sizeof geometry, &written) == OK);
    assert(written == sizeof geometry && geometry[0] == 8 && geometry[4] == 96 && geometry[20] == 58);
    WsSliceOperation input = {.abi_version = 8, .struct_size = 64, .tag = 2, .words = {0}};
    assert(ws_slice_apply(slice, &input) == OK); /* Pause commits without a physical tick. */
    input.tag = 3; input.words[0] = 1000000;
    assert(ws_slice_apply(slice, &input) == OK);
    uint8_t dynamic[1760];
    assert(ws_slice_fill_snapshot(slice, dynamic, sizeof dynamic, &written) == OK);
    assert(written == sizeof dynamic && dynamic[0] == 8 && dynamic[4] == 128 && dynamic[48] == 30 && dynamic[56] == 4);
    assert(dynamic[8] == 0 && dynamic[44] == 0); /* Paused elapsed time adds no ticks. */
    WsCalendarSky slice_sky = {.abi_version = 8, .struct_size = 160};
    assert(ws_slice_get_calendar_sky(slice, &slice_sky) == OK);
    assert(slice_sky.tick == 0 && slice_sky.sim_time_us == time && slice_sky.scale_tag == 0);
    before_hash = slice_sky.state_hash;
    assert(ws_slice_get_calendar_sky(slice, &slice_sky) == OK && slice_sky.state_hash == before_hash);
    uint64_t resident_id = 0, building_id = 0;
    for (unsigned i = 0; i < 8; ++i) {
        resident_id |= (uint64_t)dynamic[128 + i] << (8 * i);
        building_id |= (uint64_t)dynamic[128 + 30 * 48 + i] << (8 * i);
    }
    uint8_t inspection[8192];
    for (unsigned target = 0; target < 2; ++target) {
        const uint64_t id = target == 0 ? resident_id : building_id;
        written = 0;
        assert(ws_slice_inspect(slice, id, NULL, 0, &written) == BUFFER_TOO_SMALL);
        assert(written >= 96 && written <= sizeof inspection);
        assert(ws_slice_inspect(slice, id, inspection, sizeof inspection, &written) == OK);
        assert(inspection[0] == 8 && inspection[4] == 96 && inspection[48] == 3 && inspection[52] == target + 1);
        assert(inspection[96] == 2);
        if (target == 0) {
            unsigned identity = 0;
            for (unsigned i=0;i<4;++i) identity |= (unsigned)inspection[64+i] << (8*i);
            const unsigned state_at=96+identity;
            assert(inspection[136] <= 1 && inspection[137] == 0 && inspection[138] == 0 && inspection[139] == 0);
            assert(inspection[state_at+56] == 100 && inspection[state_at+57] == 0 && inspection[state_at+58] == 0 && inspection[state_at+59] == 0);
            for (unsigned i=0;i<4;++i) assert(inspection[state_at+60+i] == 0);
        }
        uint64_t queried_id = 0, queried_hash = 0;
        for (unsigned i = 0; i < 8; ++i) {
            queried_id |= (uint64_t)inspection[40 + i] << (8 * i);
            queried_hash |= (uint64_t)inspection[24 + i] << (8 * i);
        }
        assert(queried_id == id && queried_hash == before_hash);
    }
    written = 123;
    assert(ws_slice_inspect(slice, 0, inspection, sizeof inspection, &written) == INVALID_ARGUMENT && written == 0);
    assert(ws_slice_get_calendar_sky(slice, &slice_sky) == OK && slice_sky.state_hash == before_hash);
    input.tag = 1; input.words[0] = 1;
    assert(ws_slice_apply(slice, &input) == OK); /* Explicit steps remain independent of pause. */
    assert(ws_slice_get_calendar_sky(slice, &slice_sky) == OK && slice_sky.tick == 1);
    input.reserved = 1;
    assert(ws_slice_apply(slice, &input) == INVALID_ARGUMENT);
    char error[256];
    assert(ws_slice_last_error(slice, error, sizeof error) > 1 && error[0] != 0);
    input.reserved=0;input.tag=5;
    uint64_t person=0;for(unsigned i=0;i<8;i++) person|=(uint64_t)dynamic[128+i]<<(8*i);
    for(unsigned i=0;i<6;i++) input.words[i]=0;
    input.words[0]=person;input.words[1]=5;input.words[2]=200000;
    assert(ws_slice_apply(slice,&input)==OK);
    assert(ws_slice_inspect(slice,person,inspection,sizeof inspection,&written)==OK);
    unsigned state_at=96;for(unsigned i=0;i<4;i++) state_at+=(unsigned)inspection[64+i]<<(8*i);
    assert(inspection[state_at+56]==55&&inspection[state_at+60]==0&&inspection[state_at+84]==1);
    uint64_t root=0;for(unsigned i=0;i<8;i++) root|=(uint64_t)inspection[state_at+96+i]<<(8*i);
    uint8_t consequences[8192];assert(ws_slice_consequences(slice,root,consequences,sizeof consequences,&written)==OK);
    assert(consequences[0]==8&&consequences[4]==96&&consequences[64]==128&&consequences[96+32]==13&&consequences[96+128+32]==5);
    input.tag=1;for(unsigned i=0;i<6;i++) input.words[i]=0;input.words[0]=2;
    assert(ws_slice_apply(slice,&input)==OK);
    assert(ws_slice_inspect(slice,person,inspection,sizeof inspection,&written)==OK);
    assert(inspection[state_at+56]==100&&inspection[state_at+60]==0&&inspection[state_at+84]==0);
    ws_slice_destroy(slice);
    ws_slice_destroy(NULL);
    return 0;
}
