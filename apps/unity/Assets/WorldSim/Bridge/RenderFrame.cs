using UnityEngine;

namespace WorldSim.Bridge
{
    /// <summary>Reusable decoded ABI-3 frame: building boxes and resident capsules as instancing matrices.</summary>
    public sealed class RenderFrame
    {
        public readonly Matrix4x4[] Buildings;
        public readonly Matrix4x4[] Residents;
        public int BuildingCount { get; internal set; }
        public int ResidentCount { get; internal set; }
        public ulong Tick { get; internal set; }
        public RenderFrame(int buildingCapacity, int residentCapacity)
        {
            Buildings = new Matrix4x4[buildingCapacity];
            Residents = new Matrix4x4[residentCapacity];
        }
    }
}
