/* An actual C consumer of the generated header and host library. */
#include "ws_ffi.h"
#include <assert.h>
#include <stddef.h>

_Static_assert(ABI_VERSION == 3, "ABI v3 required");
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

int main(void) {
    assert(ws_abi_version() == 3);
    WsConfig config = {2, 32, 7, 0, 1, 0};
    assert(ws_create(&config) == NULL); /* Old client must be refused. */
    config.abi_version = 3;
    WsHandle *world = ws_create(&config);
    assert(world != NULL);
    const uint8_t old_batch[] = {1,0,0,0,0,0,0,0};
    assert(ws_submit(world, old_batch, sizeof old_batch) == VERSION_MISMATCH);
    const uint8_t pause[] = {2,0,0,0,1,0,0,0,1,0,0,0,12,0,0,0,1,0,0,0,0,0,0,0,0,0,0,0};
    assert(ws_submit(world, pause, sizeof pause) == OK);
    WsControlState state = {3, 16, 999, 999};
    assert(ws_get_control_state(world, &state) == OK);
    assert(state.scale_tag == 1); /* Still pending. */
    assert(ws_advance(world, 0) == OK);
    assert(ws_get_control_state(world, &state) == OK);
    assert(state.abi_version == 3 && state.struct_size == 16);
    assert(state.scale_tag == 0 && state.reserved == 0);
    assert(ws_advance(world, 1) == OK); /* Explicit tick while paused. */
    uint8_t snapshot[24 + 32 * 10]; /* 5 buildings + 5 residents */
    size_t written = 0;
    assert(ws_fill_render_snapshot(world, snapshot, sizeof snapshot, &written) == OK);
    assert(written == sizeof snapshot && snapshot[0] == 3 && snapshot[8] == 1 && snapshot[16] == 10 && snapshot[32] == 1 && snapshot[24 + 5 * 32 + 8] == 2);
    ws_destroy(world);
    return 0;
}
