using System;
using System.Collections;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Runtime.InteropServices;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.Rendering.Universal;
using UnityEngine.TestTools;
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
            var label = new NativeUtcLabel { Version = 5, Size = 40, Year = 2026, Month = 6, Day = 21, Hour = 17 };
            CollectionAssert.AreEqual(Fixture("utc-label-v5.hex"), NativeBytes(label));
            using (var world = new NativeWorld(7, 6, NativeWorld.UtcToSim(2026, 6, 21, 17, 0, 0)))
            {
                var sky = world.GetCalendarSky();
                ulong realHash = sky.StateHash;
                // The shared transport fixture uses a deliberate zero hash
                // sentinel; canonical hash/query invariance is tested separately.
                sky.StateHash = 0;
                CollectionAssert.AreEqual(Fixture("calendar-sky-v5.hex"), NativeBytes(sky));
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
            foreach (uint old in new uint[] { 1, 2, 3, 4 })
            {
                Assert.Throws<InvalidOperationException>(() => NativeWorld.RequireVersion(old));
                var config = new NativeConfig { Version = old, Size = 32, Seed = 7, ResidentsPerBuilding = 1 };
                var handle = NativeMethods.Create(in config);
                if (handle != IntPtr.Zero) NativeMethods.Destroy(handle);
                Assert.That(handle, Is.EqualTo(IntPtr.Zero));
            }
        }

        [Test]
        public void PureUtcConfigurationPreservesLeapLabelsAndDoesNotSnapGrid()
        {
            Assert.That(NativeWorld.UtcToSim(2000, 1, 1, 12, 0, 0), Is.Zero);
            long leap = NativeWorld.UtcToSim(2016, 12, 31, 23, 59, 60);
            Assert.That(NativeWorld.UtcToSim(2017, 1, 1, 0, 0, 0) - leap, Is.EqualTo(1000000));
            using (var world = new NativeWorld(7, 1, leap))
            {
                var sky = world.GetCalendarSky();
                Assert.That(sky.UtcSecond, Is.EqualTo(60));
                Assert.That(sky.LocalSecond, Is.EqualTo(60));
                Assert.That(sky.LocalHour, Is.EqualTo(18));
                Assert.That(sky.UtcOffsetSeconds, Is.EqualTo(-18000));
                world.Advance(10);
                Assert.That(world.GetCalendarSky().UtcYear, Is.EqualTo(2017));
            }
            Assert.That(NativeWorld.UtcToSim(2000, 1, 1, 12, 0, 0, 1), Is.EqualTo(1));
            Assert.Throws<InvalidOperationException>(() => new NativeWorld(7, 1, 1));
            Assert.Throws<InvalidOperationException>(() => NativeWorld.UtcToSim(2023, 2, 29, 0, 0, 0));
            Assert.Throws<InvalidOperationException>(() => NativeWorld.UtcToSim(2026, 257, 1, 0, 0, 0));
            Assert.Throws<InvalidOperationException>(() => NativeWorld.UtcToSim(2026, 6, 21, 17, 0, 60));
        }

        [Test]
        public void CalendarSnapshotPairIsReadOnlyAndPendingCommandsStayPending()
        {
            long start = NativeWorld.UtcToSim(2026, 6, 21, 17, 0, 0);
            using (var world = new NativeWorld(7, 6, start))
            {
                var frame = new RenderFrame(5, 30);
                var initial = world.GetCalendarSky();
                world.SubmitTimeScale(0);
                Assert.That(world.GetCalendarSky().StateHash, Is.EqualTo(initial.StateHash));
                Assert.That(world.GetCalendarSky().ScaleTag, Is.EqualTo(1));
                world.FillSnapshot(frame);
                Assert.That(frame.Sky.Tick, Is.EqualTo(frame.Tick));
                Assert.That(frame.Sky.SimTimeMicroseconds, Is.EqualTo(start));
                Assert.That(frame.Sky.LocalHour, Is.EqualTo(13));
                Assert.That(frame.Sky.UtcOffsetSeconds, Is.EqualTo(-14400));
                Assert.That(frame.Sky.ScaleTag, Is.EqualTo(1));
                world.FlushCommands();
                Assert.That(world.GetCalendarSky().ScaleTag, Is.Zero);
                Assert.That(world.GetCalendarSky().Tick, Is.Zero);
                world.SubmitTimeScale(100); world.FlushCommands();
                world.Advance(2); world.FillSnapshot(frame);
                Assert.That(frame.Sky.Tick, Is.EqualTo(frame.Tick).And.EqualTo(2));
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
                using (var day = new NativeWorld(7, 1, NativeWorld.UtcToSim(2026, 6, 21, 17, 0, 0)))
                using (var night = new NativeWorld(7, 1, NativeWorld.UtcToSim(2026, 12, 21, 2, 0, 0)))
                using (var unsupported = new NativeWorld(7, 1, long.MaxValue / 100000 * 100000))
                {
                    var sky = day.GetCalendarSky(); display.Apply(sky);
                    Assert.That(light.enabled, Is.True);
                    Assert.That(Vector3.Dot(light.transform.forward, -sky.TowardSun.normalized), Is.GreaterThan(0.999999f));
                    StringAssert.Contains("UTC 2026-06-21 17:00:00", display.Text);
                    StringAssert.Contains("NY 2026-06-21 13:00:00 UTC-04:00", display.Text);
                    day.SubmitTimeScale(100); day.FlushCommands(); display.Apply(day.GetCalendarSky());
                    StringAssert.Contains("100×", display.Text);
                    day.SubmitTimeScale(0); day.FlushCommands(); display.Apply(day.GetCalendarSky());
                    StringAssert.Contains("Paused", display.Text);
                    display.Apply(night.GetCalendarSky()); Assert.That(light.enabled, Is.False);
                    var missing = unsupported.GetCalendarSky();
                    Assert.That(missing.Supported, Is.False);
                    Assert.That(missing.SunEast, Is.Zero); Assert.That(missing.LocalYear, Is.Zero);
                    display.Apply(missing); Assert.That(light.enabled, Is.False);
                    StringAssert.Contains("unavailable", display.Text);
                }
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
                using (var world = new NativeWorld(7, 6, NativeWorld.UtcToSim(2026, 6, 21, 17, 0, 0)))
                {
                    var frame = new RenderFrame(5, 30);
                    for (int i = 0; i < 20; i++) { world.FillSnapshot(frame); display.Apply(frame.Sky); }
                    long before = GC.GetAllocatedBytesForCurrentThread();
                    for (int i = 0; i < 100; i++) { world.FillSnapshot(frame); display.Apply(frame.Sky); }
                    long allocated = GC.GetAllocatedBytesForCurrentThread() - before;
                    Assert.That(allocated, Is.Zero, "Calendar pairing and unchanged-label presentation must not allocate per frame.");
                }
            }
            finally { Object.DestroyImmediate(go); }
        }

        [UnityTearDown]
        public IEnumerator ExitAfterAnyFailure()
        {
            if (Application.isPlaying) yield return new ExitPlayMode();
        }

        [UnityTest]
        public IEnumerator ActualHudControlsPauseAndHundredTimesKeepUpAtSlicePopulation()
        {
            yield return new EnterPlayMode();
            // Create this iterator and its captured-lambda display class only
            // after EnterPlayMode's domain reload. Compiler-generated closure
            // objects created before that reload are not serialized by Unity.
            yield return ExerciseHudAndMeasure();
            yield return new ExitPlayMode();
        }

        static IEnumerator ExerciseHudAndMeasure()
        {
            // Load inside Play Mode as well: a preceding coroutine test's
            // ExitPlayMode can restore the Test Runner's empty temporary scene
            // across the domain reload. Do not rely on the previous test's scene.
            var loading = UnityEngine.SceneManagement.SceneManager.LoadSceneAsync("Bootstrap", UnityEngine.SceneManagement.LoadSceneMode.Single);
            while (!loading.isDone) yield return null;
            yield return null;
            var demo = Object.FindFirstObjectByType<BridgeDemo>();
            Assert.That(demo, Is.Not.Null);
            Assert.That(demo.Running, Is.True);
            Assert.That(demo.CurrentFrame.ResidentCount, Is.EqualTo(30), "This measurement is the toy backend at the maximum real-slice population.");
            var light = Object.FindFirstObjectByType<Light>();
            foreach (uint speed in new uint[] { 0, 1, 10, 100, 0 })
            {
                demo.SetSpeed(speed);
                double deadline = Time.realtimeSinceStartupAsDouble + 3;
                while (demo.CommittedScale != speed && Time.realtimeSinceStartupAsDouble < deadline) yield return null;
                Assert.That(demo.CommittedScale, Is.EqualTo(speed));
                Assert.That(demo.CurrentFrame.Sky.Tick, Is.EqualTo(demo.CurrentFrame.Tick));
                if (speed == 0)
                {
                    ulong tick = demo.Tick; long time = demo.CurrentFrame.Sky.SimTimeMicroseconds;
                    double until = Time.realtimeSinceStartupAsDouble + 0.15;
                    while (Time.realtimeSinceStartupAsDouble < until) yield return null;
                    Assert.That(demo.Tick, Is.EqualTo(tick));
                    Assert.That(demo.CurrentFrame.Sky.SimTimeMicroseconds, Is.EqualTo(time));
                    StringAssert.Contains("Paused", demo.CalendarText);
                }
            }
            Assert.Throws<ArgumentOutOfRangeException>(() => demo.SetSpeed(2));
            Assert.That(light.enabled, Is.True);
            // Measure actual playing Editor frames with graphics, uncapped to
            // expose available headroom. Device vsync/thermal evidence is#72/#76.
            int previousTarget = Application.targetFrameRate; Application.targetFrameRate = -1;
            var camera = Camera.main;
            var target = new RenderTexture(960, 540, 24); target.Create();
            var fence = new Texture2D(1, 1, TextureFormat.RGB24, false);
            var request = new UniversalRenderPipeline.SingleCameraRequest { destination = target };
            Assert.That(RenderPipeline.SupportsRenderRequest(camera, request), Is.True);
            camera.targetTexture = target;
            Action renderAndFence = () =>
            {
                RenderPipeline.SubmitRenderRequest(camera, request);
                var old = RenderTexture.active; RenderTexture.active = target;
                // Synchronous1-pixel readback waits for all preceding GPU work;
                // every measured iteration includes a completed960×540 frame.
                fence.ReadPixels(new Rect(0, 0, 1, 1), 0, 0, false);
                RenderTexture.active = old;
            };
            string path = Path.Combine(Path.GetTempPath(), "worldsim71-editor-100x.csv");
            var c = CultureInfo.InvariantCulture;
            try
            {
                using (var csv = new StreamWriter(path))
                {
                    csv.WriteLine("backend,residents,scale,wall_s,tick,frame_ms,fill_ms,advance_ms_mean,advance_ms_max,render_fenced_ms");
                    foreach (uint speed in new uint[] { 1, 100 })
                    {
                        demo.SetSpeed(speed);
                        double warm = Time.realtimeSinceStartupAsDouble + 2;
                        while (Time.realtimeSinceStartupAsDouble < warm) { yield return null; renderAndFence(); }
                        Assert.That(demo.CommittedScale, Is.EqualTo(speed));
                        double begin = Time.realtimeSinceStartupAsDouble, previous = begin;
                        ulong firstTick = demo.Tick;
                        var frames = new List<double>();
                        while (Time.realtimeSinceStartupAsDouble - begin < 10)
                        {
                            yield return null;
                            double renderBegin = Time.realtimeSinceStartupAsDouble;
                            renderAndFence();
                            double now = Time.realtimeSinceStartupAsDouble;
                            double ms = (now - previous) * 1000; previous = now; frames.Add(ms);
                            csv.WriteLine(string.Join(",", "toy", 30, speed, (now - begin).ToString("F6", c), demo.Tick,
                                ms.ToString("F6", c), demo.LastFillMs.ToString("F6", c), demo.AdvanceMeanMs.ToString("F6", c), demo.AdvanceMaxMs.ToString("F6", c), ((now - renderBegin) * 1000).ToString("F6", c)));
                        }
                        double seconds = Time.realtimeSinceStartupAsDouble - begin;
                        double ticksPerSecond = (demo.Tick - firstTick) / seconds;
                        double fps = frames.Count / seconds;
                        Debug.Log($"VS007 Editor toy30 scale{speed}: fps={fps:F3}, ticks/s={ticksPerSecond:F3}, csv={path}");
                        Assert.That(fps, Is.GreaterThanOrEqualTo(60), "Actual Editor graphics loop must meet60fps at both scales.");
                        Assert.That(ticksPerSecond, Is.InRange(speed * 10 * 0.98, speed * 10 * 1.02), "Worker must keep up without lost whole-step budgets.");
                    }
                }
                var image = new Texture2D(960, 540, TextureFormat.RGB24, false);
                try
                {
                    var old = RenderTexture.active; RenderTexture.active = target;
                    image.ReadPixels(new Rect(0, 0, 960, 540), 0, 0); image.Apply(); RenderTexture.active = old;
                    int cyan = 0;
                    foreach (var pixel in image.GetPixels32()) if (pixel.g > pixel.r * 2 && pixel.b > pixel.r * 2 && pixel.g > 100) cyan++;
                    File.WriteAllBytes(Path.Combine(Path.GetTempPath(), "worldsim71-editor-100x.png"), image.EncodeToPNG());
                    Assert.That(cyan, Is.GreaterThan(100), "Measured render requests must contain actual native geometry.");
                }
                finally { Object.Destroy(image); }
            }
            finally
            {
                Application.targetFrameRate = previousTarget;
                camera.targetTexture = null;
                Object.Destroy(fence); Object.Destroy(target);
            }
        }
    }
}
