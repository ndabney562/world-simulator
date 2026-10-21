using System;
using System.Runtime.InteropServices;
using UnityEngine;

namespace WorldSim.Bridge
{
    [StructLayout(LayoutKind.Sequential)]
    internal struct NativeUtcLabel
    {
        internal uint Version, Size;
        internal int Year;
        internal uint Month, Day, Hour, Minute, Second, Microsecond, Reserved;
    }

    /// <summary>ABI5 committed calendar/sky value. Core owns all time, timezone and solar calculations.</summary>
    [StructLayout(LayoutKind.Sequential)]
    public struct CalendarSky
    {
        public uint Version, Size;
        public ulong Tick;
        public long SimTimeMicroseconds;
        public ulong StateHash;
        public int UtcYear;
        public uint UtcMonth, UtcDay, UtcHour, UtcMinute, UtcSecond, UtcMicrosecond;
        public int LocalYear;
        public uint LocalMonth, LocalDay, LocalHour, LocalMinute, LocalSecond, LocalMicrosecond;
        public int UtcOffsetSeconds;
        public uint ScaleTag, SolarVersion, NyPolicyVersion, Flags, LeapTableVersion;
        public ulong SiteDatasetVersion;
        public double SunEast, SunNorth, SunUp, AzimuthDegrees, ElevationDegrees;
        public bool Supported => (Flags & 1) != 0;
        /// <summary>Presentation-only ENU→Unity axis swap. This vector points toward the sun.</summary>
        public Vector3 TowardSun => new Vector3((float)SunEast, (float)SunUp, (float)SunNorth);

        static bool Finite(double v) => !double.IsNaN(v) && !double.IsInfinity(v);
        internal void Validate()
        {
            NativeWorld.RequireVersion(Version);
            if (Size != 160 || (Flags & ~1u) != 0 ||
                (ScaleTag != 0 && ScaleTag != 1 && ScaleTag != 10 && ScaleTag != 100) ||
                SolarVersion != 1 || NyPolicyVersion != 1 || LeapTableVersion != 1 || SiteDatasetVersion == 0 ||
                UtcMonth < 1 || UtcMonth > 12 || UtcDay < 1 || UtcDay > 31 || UtcHour > 23 || UtcMinute > 59 || UtcSecond > 60 || UtcMicrosecond > 999999 ||
                !Finite(SunEast) || !Finite(SunNorth) || !Finite(SunUp) || !Finite(AzimuthDegrees) || !Finite(ElevationDegrees))
                throw new InvalidOperationException("Invalid native calendar/sky record.");
            if (Supported)
            {
                double norm = SunEast * SunEast + SunNorth * SunNorth + SunUp * SunUp;
                if (UtcYear < 2000 || UtcYear > 2099 || LocalMonth < 1 || LocalMonth > 12 || LocalDay < 1 || LocalDay > 31 || LocalHour > 23 || LocalMinute > 59 || LocalSecond > 60 || LocalMicrosecond > 999999 ||
                    (UtcOffsetSeconds != -18000 && UtcOffsetSeconds != -14400) || AzimuthDegrees < 0 || AzimuthDegrees >= 360 || ElevationDegrees < -90 || ElevationDegrees > 90 || Math.Abs(norm - 1) > 1e-12)
                    throw new InvalidOperationException("Invalid supported native calendar/sky values.");
            }
            else if (LocalYear != 0 || LocalMonth != 0 || LocalDay != 0 || LocalHour != 0 || LocalMinute != 0 || LocalSecond != 0 || LocalMicrosecond != 0 || UtcOffsetSeconds != 0 ||
                SunEast != 0 || SunNorth != 0 || SunUp != 0 || AzimuthDegrees != 0 || ElevationDegrees != 0)
                throw new InvalidOperationException("Unavailable calendar/sky fields must be zero.");
        }
    }
}
