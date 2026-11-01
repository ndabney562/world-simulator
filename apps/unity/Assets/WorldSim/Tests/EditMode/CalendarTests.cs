using System;
using System.IO;
using System.Runtime.InteropServices;
using NUnit.Framework;
using UnityEngine;
using WorldSim.Bridge;
using WorldSim.Rendering;
using Object = UnityEngine.Object;

namespace WorldSim.Tests.EditMode
{
    public sealed class CalendarTests
    {
        static byte[] Fixture(string name)
        {
            string hex = File.ReadAllText(Path.Combine(Application.dataPath, "../../../core/crates/ws-ffi/tests/fixtures/" + name)).Trim();
            var bytes = new byte[hex.Length / 2];
            for (int i = 0; i < bytes.Length; i++) bytes[i] = Convert.ToByte(hex.Substring(i * 2, 2), 16);
            return bytes;
        }
        static byte[] NativeBytes<T>(T value) where T : struct
        {
            int size = Marshal.SizeOf<T>(); var bytes = new byte[size];
            IntPtr storage = Marshal.AllocHGlobal(size);
            try { Marshal.StructureToPtr(value, storage, false); Marshal.Copy(storage, bytes, 0, size); return bytes; }
            finally { Marshal.FreeHGlobal(storage); }
        }
        [Test]
        public void SharedUtcAndSkyByteFixturesMatchNativeLayoutAndInvalidRecordsAreRejected()
        {
            var label = new NativeUtcLabel { Version = NativeWorld.AbiVersion, Size = 40, Year = 2026, Month = 6, Day = 21, Hour = 17 };
            CollectionAssert.AreEqual(Fixture("utc-label-v11.hex"), NativeBytes(label));
            using (var world = new NativeSlice(7, NativeWorld.UtcToSim(2026, 6, 21, 17, 0, 0)))
            {
                var sky = world.GetCalendarSky();
                ulong realHash = sky.StateHash;
                // The shared transport fixture uses a deliberate zero hash
                // sentinel; canonical hash/query invariance is tested separately.
                sky.StateHash = 0;
                CollectionAssert.AreEqual(Fixture("calendar-sky-v11.hex"), NativeBytes(sky));
                Assert.That(world.GetCalendarSky().StateHash, Is.EqualTo(realHash));
                sky = world.GetCalendarSky(); sky.Version = 3;
                Assert.Throws<InvalidOperationException>(() => sky.Validate());
                sky = world.GetCalendarSky(); sky.Flags = 2;
                Assert.Throws<InvalidOperationException>(() => sky.Validate());
                sky = world.GetCalendarSky(); sky.SunEast = double.NaN;
                Assert.Throws<InvalidOperationException>(() => sky.Validate());
                sky = world.GetCalendarSky(); sky.UtcYear = 2100;
                Assert.Throws<InvalidOperationException>(() => sky.Validate());
            }
        }
        [Test]
        public void CalendarAndUtcLayoutsMatchEveryNativeOffset()
        {
            Assert.That(Marshal.SizeOf<NativeUtcLabel>(), Is.EqualTo(40));
            string[] utc = { "Version", "Size", "Year", "Month", "Day", "Hour", "Minute", "Second", "Microsecond", "Reserved" };
            for (int i = 0; i < utc.Length; i++) Assert.That(Marshal.OffsetOf<NativeUtcLabel>(utc[i]).ToInt32(), Is.EqualTo(i * 4));
            Assert.That(Marshal.SizeOf<CalendarSky>(), Is.EqualTo(160));
            string[] names = { "Version", "Size", "Tick", "SimTimeMicroseconds", "StateHash", "UtcYear", "UtcMonth", "UtcDay", "UtcHour", "UtcMinute", "UtcSecond", "UtcMicrosecond", "LocalYear", "LocalMonth", "LocalDay", "LocalHour", "LocalMinute", "LocalSecond", "LocalMicrosecond", "UtcOffsetSeconds", "ScaleTag", "SolarVersion", "NyPolicyVersion", "Flags", "LeapTableVersion", "SiteDatasetVersion", "SunEast", "SunNorth", "SunUp", "AzimuthDegrees", "ElevationDegrees" };
            int[] offsets = { 0, 4, 8, 16, 24, 32, 36, 40, 44, 48, 52, 56, 60, 64, 68, 72, 76, 80, 84, 88, 92, 96, 100, 104, 108, 112, 120, 128, 136, 144, 152 };
            for (int i = 0; i < names.Length; i++) Assert.That(Marshal.OffsetOf<CalendarSky>(names[i]).ToInt32(), Is.EqualTo(offsets[i]), names[i]);
        }

        [Test]
        public void PureUtcConfigurationPreservesLeapLabelsAndDoesNotSnapGrid()
        {
            Assert.That(NativeWorld.UtcToSim(2000, 1, 1, 12, 0, 0), Is.Zero);
            long leap = NativeWorld.UtcToSim(2016, 12, 31, 23, 59, 60);
            Assert.That(NativeWorld.UtcToSim(2017, 1, 1, 0, 0, 0) - leap, Is.EqualTo(1000000));
            using (var world = new NativeSlice(7, leap))
            {
                var sky = world.GetCalendarSky();
                Assert.That(sky.UtcSecond, Is.EqualTo(60));
                Assert.That(sky.LocalSecond, Is.EqualTo(60));
                Assert.That(sky.LocalHour, Is.EqualTo(18));
                Assert.That(sky.UtcOffsetSeconds, Is.EqualTo(-18000));
                world.Steps(10);
                Assert.That(world.GetCalendarSky().UtcYear, Is.EqualTo(2017));
            }
            Assert.That(NativeWorld.UtcToSim(2000, 1, 1, 12, 0, 0, 1), Is.EqualTo(1));
            Assert.Throws<InvalidOperationException>(() => new NativeSlice(7, 1));
            Assert.Throws<InvalidOperationException>(() => NativeWorld.UtcToSim(2023, 2, 29, 0, 0, 0));
            Assert.Throws<InvalidOperationException>(() => NativeWorld.UtcToSim(2026, 257, 1, 0, 0, 0));
            Assert.Throws<InvalidOperationException>(() => NativeWorld.UtcToSim(2026, 6, 21, 17, 0, 60));
        }

        [Test]
        public void CalendarQueryIsReadOnlyAndPairsWithTheFilledFrame()
        {
            long start = NativeWorld.UtcToSim(2026, 6, 21, 17, 0, 0);
            using (var world = new NativeSlice(7, start))
            {
                var frame = new SliceFrame();
                var initial = world.GetCalendarSky();
                Assert.That(world.GetCalendarSky().StateHash, Is.EqualTo(initial.StateHash));
                Assert.That(initial.ScaleTag, Is.EqualTo(1));
                world.Fill(frame, out _);
                Assert.That(frame.Sky.Tick, Is.Zero);
                Assert.That(frame.Sky.SimTimeMicroseconds, Is.EqualTo(start));
                Assert.That(frame.Sky.LocalHour, Is.EqualTo(13));
                Assert.That(frame.Sky.UtcOffsetSeconds, Is.EqualTo(-14400));
                world.SetScale(0);
                Assert.That(world.GetCalendarSky().ScaleTag, Is.Zero);
                Assert.That(world.GetCalendarSky().Tick, Is.Zero);
                world.SetScale(100);
                world.Steps(2); world.Fill(frame, out _);
                Assert.That(frame.Sky.Tick, Is.EqualTo(2));
                Assert.That(frame.Sky.ScaleTag, Is.EqualTo(100));
                Assert.That(frame.Sky.SimTimeMicroseconds, Is.EqualTo(start + 200000));
                world.Dispose();
                Assert.Throws<ObjectDisposedException>(() => world.GetCalendarSky());
            }
        }

        [Test]
        public void SunLightAndHudPresentCoreValuesIncludingNightAndUnavailableStatus()
        {
            var go = new GameObject("Calendar test light");
            try
            {
                var light = go.AddComponent<Light>(); light.type = LightType.Directional;
                var display = new CalendarPresentation(light);
                using (var day = new NativeSlice(7, NativeWorld.UtcToSim(2026, 6, 21, 17, 0, 0)))
                using (var night = new NativeSlice(7, NativeWorld.UtcToSim(2026, 12, 21, 2, 0, 0)))
                {
                    var sky = day.GetCalendarSky(); display.Apply(sky);
                    Assert.That(light.enabled, Is.True);
                    Assert.That(Vector3.Dot(light.transform.forward, -sky.TowardSun.normalized), Is.GreaterThan(0.999999f));
                    StringAssert.Contains("UTC 2026-06-21 17:00:00", display.Text);
                    StringAssert.Contains("NY 2026-06-21 13:00:00 UTC-04:00", display.Text);
                    day.SetScale(100); display.Apply(day.GetCalendarSky());
                    StringAssert.Contains("100×", display.Text);
                    day.SetScale(0); display.Apply(day.GetCalendarSky());
                    StringAssert.Contains("Paused", display.Text);
                    display.Apply(night.GetCalendarSky()); Assert.That(light.enabled, Is.False);
                    var missing = day.GetCalendarSky(); missing.Flags = 0; missing.SunEast = 0; missing.SunNorth = 0; missing.SunUp = 0;
                    Assert.That(missing.Supported, Is.False);
                    display.Apply(missing); Assert.That(light.enabled, Is.False);
                    StringAssert.Contains("unavailable", display.Text);
                }
                // The slice refuses an unsupported calendar start instead of reporting it.
                Assert.Throws<InvalidOperationException>(() => new NativeSlice(7, long.MaxValue / 100000 * 100000));
            }
            finally { Object.DestroyImmediate(go); }
        }

        [Test]
        public void WarmedCalendarFillAndPresentationReuseManagedStorage()
        {
            var go = new GameObject("Calendar allocation light");
            try
            {
                var light = go.AddComponent<Light>(); light.type = LightType.Directional;
                var display = new CalendarPresentation(light);
                using (var world = new NativeSlice(7, NativeWorld.UtcToSim(2026, 6, 21, 17, 0, 0)))
                {
                    var frame = new SliceFrame();
                    for (int i = 0; i < 20; i++) { world.Fill(frame, out _); display.Apply(frame.Sky); }
                    long before = GC.GetAllocatedBytesForCurrentThread();
                    for (int i = 0; i < 100; i++) { world.Fill(frame, out _); display.Apply(frame.Sky); }
                    long allocated = GC.GetAllocatedBytesForCurrentThread() - before;
                    Assert.That(allocated, Is.Zero, "Calendar pairing and unchanged-label presentation must not allocate per frame.");
                }
            }
            finally { Object.DestroyImmediate(go); }
        }
    }
}
