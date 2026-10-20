using UnityEngine;

namespace WorldSim.Bridge
{
    /// <summary>Reusable ABI4 frame: building/resident matrices and a calendar/sky value from the same tick.</summary>
    public sealed class RenderFrame
    {
        public readonly Matrix4x4[] Buildings;
        public readonly Matrix4x4[] Residents;
        public int BuildingCount { get; internal set; }
        public int ResidentCount { get; internal set; }
        public ulong Tick { get; internal set; }
        public CalendarSky Sky { get; internal set; }
        public RenderFrame(int buildingCapacity, int residentCapacity)
        {
            Buildings = new Matrix4x4[buildingCapacity];
            Residents = new Matrix4x4[residentCapacity];
        }
    }
}
