using System.Collections;
using System.IO;
using NUnit.Framework;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.TestTools;
using WorldSim.Rendering;

namespace WorldSim.Tests.EditMode
{
    public sealed class BridgePlayTests
    {
        [Test]
        public void DemoInputBackendSupportsLegacyMouseAndTouch()
        {
#if ENABLE_LEGACY_INPUT_MANAGER
            Assert.DoesNotThrow(() => { var position = Input.mousePosition; var count = Input.touchCount; });
#else
            Assert.Fail("The demo uses UnityEngine.Input; select Input Manager in Active Input Handling.");
#endif
        }

        [TestCase(16f / 9f, 9f / 16f)]
        [TestCase(9f / 16f, 16f / 9f)]
        public void OverviewFitsEveryBuildingAndRotationKeepsZoom(float aspect, float rotatedAspect)
        {
            var go = new GameObject("Framing regression camera");
            try
            {
                var camera = go.AddComponent<Camera>();
                camera.aspect = aspect;
                using (var world = new WorldSim.Bridge.NativeWorld(7, 1))
                {
                    var frame = new WorldSim.Bridge.RenderFrame(5, 5);
                    world.FillSnapshot(frame);
                    var orbit = new OrbitCamera(camera, Vector3.zero);
                    Assert.That(orbit.FitsAspect, Is.False);
                    BridgeDemo.FrameBuildings(orbit, frame, false); // the demo's own framing path
                    Assert.That(orbit.FitsAspect, Is.True);
                    Assert.That(orbit.Distance, Is.EqualTo(orbit.FitDistance));
                    AssertBuildingCornersVisible(camera, frame);
                    orbit.Distance *= 2f;
                    orbit.Update(true);
                    AssertBuildingCornersVisible(camera, frame);

                    // The user zooms in, then rotates the phone: the refit keeps their zoom relative to the new fit.
                    orbit.Distance = orbit.FitDistance * 0.5f;
                    orbit.Update(true);
                    float landscapeFit = orbit.FitDistance;
                    camera.aspect = rotatedAspect;
                    Assert.That(orbit.FitsAspect, Is.False);
                    BridgeDemo.FrameBuildings(orbit, frame, true);
                    Assert.That(orbit.FitsAspect, Is.True);
                    Assert.That(orbit.FitDistance, Is.Not.EqualTo(landscapeFit).Within(1f), "Refit must use the new aspect");
                    Assert.That(orbit.Distance / orbit.FitDistance, Is.EqualTo(0.5f).Within(0.0001f));

                    // The refit distance itself frames every building at the rotated aspect.
                    orbit.Distance = orbit.FitDistance;
                    orbit.Update(true);
                    AssertBuildingCornersVisible(camera, frame);
                }
            }
            finally { Object.DestroyImmediate(go); }
        }

        static void AssertBuildingCornersVisible(Camera camera, WorldSim.Bridge.RenderFrame frame)
        {
            for (int building = 0; building < frame.BuildingCount; building++)
                for (int corner = 0; corner < 8; corner++)
                {
                    var local = new Vector3((corner & 1) == 0 ? -0.5f : 0.5f,
                        (corner & 2) == 0 ? -0.5f : 0.5f, (corner & 4) == 0 ? -0.5f : 0.5f);
                    var p = camera.WorldToViewportPoint(frame.Buildings[building].MultiplyPoint3x4(local));
                    Assert.That(p.x, Is.InRange(0.01f, 0.99f), $"Building {building} horizontal framing");
                    Assert.That(p.y, Is.InRange(0.01f, 0.99f), $"Building {building} vertical framing");
                    Assert.That(p.z, Is.InRange(camera.nearClipPlane, camera.farClipPlane), $"Building {building} clipping");
                }
        }

        [Test]
        public void LongStallDropsExcessWholeSteps()
        {
            double remainder=0;
            Assert.That(BridgeDemo.ConsumeHostElapsed(ref remainder,60),Is.EqualTo(100));
            Assert.That(remainder,Is.GreaterThanOrEqualTo(0).And.LessThan(0.1));
            for(int frame=0;frame<10;frame++)
                Assert.That(BridgeDemo.ConsumeHostElapsed(ref remainder,0),Is.Zero);
        }
        [Test]
        public void CatchUpPreservesOnlySubStepRemainder()
        {
            double remainder=0;
            Assert.That(BridgeDemo.ConsumeHostElapsed(ref remainder,60.025),Is.EqualTo(100));
            Assert.That(remainder,Is.EqualTo(0.025).Within(0.000000001));
            Assert.That(BridgeDemo.ConsumeHostElapsed(ref remainder,0.08),Is.EqualTo(1));
            Assert.That(remainder,Is.EqualTo(0.005).Within(0.000000001));
            Assert.That(BridgeDemo.ConsumeHostElapsed(ref remainder,0),Is.Zero);
        }
        [Test]
        public void HostUsesCommittedScaleOnceAndRetainsPausedFraction()
        {
            double remainder=0.025;
            Assert.That(BridgeDemo.ConsumeHostElapsed(ref remainder,60,0),Is.Zero);
            Assert.That(remainder,Is.EqualTo(0.025).Within(0.000000001));
            Assert.That(BridgeDemo.ConsumeHostElapsed(ref remainder,0.01,10),Is.EqualTo(1));
            Assert.That(remainder,Is.EqualTo(0.025).Within(0.000000001));
            Assert.That(BridgeDemo.ConsumeHostElapsed(ref remainder,0.01,100),Is.EqualTo(10));
            Assert.That(BridgeDemo.ConsumeHostElapsed(ref remainder,60,10),Is.EqualTo(100));
            Assert.That(BridgeDemo.ConsumeHostElapsed(ref remainder,0,1),Is.Zero);
        }
        [Test]
        public void WorkerThreadAdvancesTheWorldAndStopsCleanly()
        {
            var host=new SimHost(7,10);
            try
            {
                var frame=new WorldSim.Bridge.RenderFrame(5,50);
                host.SetScale(10);
                var clock=System.Diagnostics.Stopwatch.StartNew();
                while(host.Steps==0 && clock.Elapsed.TotalSeconds<3) System.Threading.Thread.Sleep(10);
                Assert.That(host.Steps,Is.GreaterThan(0));
                Assert.That(host.Failure,Is.Null);
                host.Fill(frame);
                Assert.That(frame.Tick,Is.GreaterThan(0));
                Assert.That(frame.ResidentCount,Is.EqualTo(50));
                host.TakeAdvanceStats(out double mean,out double max);
                Assert.That(mean,Is.GreaterThanOrEqualTo(0));Assert.That(max,Is.GreaterThanOrEqualTo(mean));
            }
            finally { host.Dispose(); }
            Assert.Throws<System.ObjectDisposedException>(()=>host.SetScale(1));
        }
        [UnityTearDown]
        public IEnumerator LeavePlayModeAfterAnyFailure()
        {
            if (Application.isPlaying) yield return new ExitPlayMode();
        }
        [UnityTest]
        public IEnumerator BootstrapRendersAndAdvancesFromNativeSnapshots()
        {
            EditorSceneManager.OpenScene("Assets/WorldSim/Scenes/Bootstrap.unity",OpenSceneMode.Single);
            yield return new EnterPlayMode();
            var demo=Object.FindFirstObjectByType<BridgeDemo>();
            var camera=Camera.main;
            Assert.That(demo,Is.Not.Null);Assert.That(demo.Running,Is.True);
            Assert.That(camera,Is.Not.Null);
            var target=new RenderTexture(960,540,24);
            camera.targetTexture=target;
            ulong initial=demo.Tick;
            double deadline=Time.realtimeSinceStartupAsDouble+1.0;
            while(Time.realtimeSinceStartupAsDouble<deadline) yield return null;
            Assert.That(demo.Tick,Is.GreaterThan(initial));
            yield return null;
            var old=RenderTexture.active;RenderTexture.active=target;
            var image=new Texture2D(960,540,TextureFormat.RGB24,false);
            image.ReadPixels(new Rect(0,0,960,540),0,0);image.Apply();
            RenderTexture.active=old;
            // Ensure this is rendered cyan geometry rather than a blank camera image.
            int total=0;
            foreach(var pixel in image.GetPixels32())
                if(pixel.g>pixel.r*2 && pixel.b>pixel.r*2 && pixel.g>100) total++;
            string path=Path.Combine(Path.GetTempPath(),"worldsim-bridge-bootstrap.png");
            File.WriteAllBytes(path,image.EncodeToPNG());
            Assert.That(total,Is.GreaterThan(100),"Native marker geometry must be visible.");
            // Every building, not just the nearest, must be rendered cyan geometry inside its own projected footprint.
            var frame=demo.CurrentFrame;
            Assert.That(frame.BuildingCount,Is.EqualTo(5));
            for(int building=0;building<frame.BuildingCount;building++)
            {
                var min=new Vector2(float.MaxValue,float.MaxValue);var max=new Vector2(float.MinValue,float.MinValue);
                for(int corner=0;corner<8;corner++)
                {
                    var local=new Vector3((corner&1)==0?-0.5f:0.5f,(corner&2)==0?-0.5f:0.5f,(corner&4)==0?-0.5f:0.5f);
                    Vector2 p=camera.WorldToScreenPoint(frame.Buildings[building].MultiplyPoint3x4(local));
                    min=Vector2.Min(min,p);max=Vector2.Max(max,p);
                }
                Assert.That(min.x>=0 && min.y>=0 && max.x<=960 && max.y<=540,Is.True,$"Building {building} is off screen ({min} to {max}); see {path}");
                int cyan=0;
                for(int y=Mathf.FloorToInt(min.y);y<Mathf.CeilToInt(max.y);y++)
                    for(int x=Mathf.FloorToInt(min.x);x<Mathf.CeilToInt(max.x);x++)
                    {
                        var pixel=(Color32)image.GetPixel(x,y);
                        if(pixel.g>pixel.r*2 && pixel.b>pixel.r*2 && pixel.g>100) cyan++;
                    }
                Assert.That(cyan,Is.GreaterThan(4),$"Building {building} must be visible; see {path}");
            }
            camera.targetTexture=null;
            Object.Destroy(image);Object.Destroy(target);
            yield return new ExitPlayMode();
        }
    }
}
