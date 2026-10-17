// Thermal state and process footprint for the TP-011 debug HUD. iOS only; read-only system queries.
#import <Foundation/Foundation.h>
#include <mach/mach.h>
#include <stdint.h>

extern "C" int ws_platform_thermal_state(void) {
    // NSProcessInfoThermalState: 0 nominal, 1 fair, 2 serious, 3 critical.
    return (int)[NSProcessInfo processInfo].thermalState;
}

extern "C" uint64_t ws_platform_footprint_bytes(void) {
    // phys_footprint is the figure Xcode's memory gauge and jetsam use.
    task_vm_info_data_t info;
    mach_msg_type_number_t count = TASK_VM_INFO_COUNT;
    if (task_info(mach_task_self(), TASK_VM_INFO, (task_info_t)&info, &count) != KERN_SUCCESS) return 0;
    return info.phys_footprint;
}
