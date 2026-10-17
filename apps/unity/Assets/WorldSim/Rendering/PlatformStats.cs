using System;
using System.Runtime.InteropServices;

namespace WorldSim.Rendering
{
    /// <summary>Thermal state and process footprint from the small iOS plugin; other platforms report unknown.</summary>
    public static class PlatformStats
    {
#if UNITY_IOS && !UNITY_EDITOR
        [DllImport("__Internal")] static extern int ws_platform_thermal_state();
        [DllImport("__Internal")] static extern ulong ws_platform_footprint_bytes();
        public static int ThermalState => ws_platform_thermal_state();
        public static ulong FootprintBytes => ws_platform_footprint_bytes();
#else
        public static int ThermalState => -1;
        public static ulong FootprintBytes => 0;
#endif
        /// <summary>NSProcessInfoThermalState: 0 nominal, 1 fair, 2 serious, 3 critical.</summary>
        public static string ThermalName(int state)
        {
            switch (state)
            {
                case 0: return "nominal";
                case 1: return "fair";
                case 2: return "serious";
                case 3: return "critical";
                default: return "n/a";
            }
        }
    }
}
