using UnityEngine;

namespace WorldSim.Bridge
{
    /// <summary>Source BIN plus optional real featured identity and disjoint mesh ranges.</summary>
    public struct SliceBuilding
    {
        public ulong Bin, Id;
        public int FirstVertex, VertexCount, FirstIndex, IndexCount;
        public float Height;
        public Vector2 Centroid;
        public bool Featured => Id != 0;
    }

    /// <summary>Validated immutable-per-world geometry, already axis-swapped to Unity.</summary>
    public sealed class SliceGeometry
    {
        public const uint RenderVersion = 1;
        public const ulong DatasetVersion = 0x85183751d4f793b9UL;
        public readonly SliceBuilding[] Buildings;
        public readonly Vector3[] Vertices, Normals;
        public readonly int[] Indices;
        public readonly ulong[] FeaturedIds;
        public readonly Bounds Bounds;
        public readonly double Latitude, Longitude, EllipsoidalHeight;
        internal SliceGeometry(SliceBuilding[] buildings, Vector3[] vertices, Vector3[] normals, int[] indices,
            ulong[] featuredIds, Bounds bounds, double latitude, double longitude, double height)
        {
            Buildings = buildings; Vertices = vertices; Normals = normals; Indices = indices;
            FeaturedIds = featuredIds; Bounds = bounds; Latitude = latitude; Longitude = longitude; EllipsoidalHeight = height;
        }
        public bool IsFeatured(ulong id)
        {
            for (int i = 0; i < FeaturedIds.Length; i++) if (FeaturedIds[i] == id) return true;
            return false;
        }
    }

    /// <summary>One retained resident; location1 alone carries real local fine geometry.</summary>
    public struct SliceResident
    {
        public ulong Id, Governing;
        public Vector3 Foot;
        public float Heading;
        public uint Detail, Activity, Location;
    }

    /// <summary>Exact core counts. Governing membership is distinct from actual occupancy.</summary>
    public struct SliceBuildingSummary
    {
        public ulong Id;
        public uint Mode, Occupancy, Governed, Sleep, Commute, Work, School, Errands, Home;
        public uint Activity(uint tag)
        {
            switch (tag)
            {
                case 0: return Sleep; case 1: return Commute; case 2: return Work;
                case 3: return School; case 4: return Errands; case 5: return Home;
                default: throw new System.ArgumentOutOfRangeException(nameof(tag));
            }
        }
    }

    /// <summary>Reused bounded real-slice frame. A failed decode leaves the previous frame intact.</summary>
    public sealed class SliceFrame
    {
        public const int MaxPopulation = 30, BuildingCount = 4;
        public readonly SliceResident[] Residents = new SliceResident[MaxPopulation];
        public readonly SliceBuildingSummary[] Buildings = new SliceBuildingSummary[BuildingCount];
        public readonly Matrix4x4[] ResidentMatrices = new Matrix4x4[MaxPopulation];
        public readonly ulong[] RenderedResidentIds = new ulong[MaxPopulation];
        public int Population { get; internal set; }
        public int RenderedResidents { get; internal set; }
        public ulong Tick { get; internal set; }
        public long SimTimeMicroseconds { get; internal set; }
        public ulong StateHash { get; internal set; }
        public uint Scale { get; internal set; }
        public uint LocalStationary { get; internal set; }
        public uint InTransit { get; internal set; }
        public uint OffMapStationary { get; internal set; }
        public uint L1 { get; internal set; }
        public uint L2 { get; internal set; }
        public CalendarSky Sky { get; internal set; }
    }
}
