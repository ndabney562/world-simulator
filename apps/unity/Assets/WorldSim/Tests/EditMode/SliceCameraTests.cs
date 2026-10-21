using System;
using System.Collections;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.TestTools;
using WorldSim.Bridge;
using WorldSim.Rendering;
using Object=UnityEngine.Object;

namespace WorldSim.Tests.EditMode
{
    public sealed class SliceCameraTests
    {
        static SliceGeometry Geometry()
        {
            using(var world=new NativeSlice(71449,NativeWorld.UtcToSim(2026,6,22,11,22,30))) return world.Geometry;
        }
        static void AssertFeaturedInView(Camera camera,SliceGeometry geometry)
        {
            foreach(var row in geometry.Buildings)
            {
                if(!row.Featured) continue;
                for(int i=row.FirstVertex;i<row.FirstVertex+row.VertexCount;i++)
                {
                    Vector3 viewport=camera.WorldToViewportPoint(geometry.Vertices[i]);
                    Assert.That(viewport.z,Is.GreaterThan(camera.nearClipPlane));
                    Assert.That(viewport.x,Is.InRange(0.025f,0.975f));Assert.That(viewport.y,Is.InRange(0.025f,0.975f));
                }
            }
        }
        [Test]
        public void DefaultFitShowsAllFeaturedFootprintsInPortraitAndWideAndMovedResizeKeepsPose()
        {
            var go=new GameObject("Slice fit test");
            try
            {
                var camera=go.AddComponent<Camera>();var geometry=Geometry();camera.aspect=16f/9;var orbit=new SliceCamera(camera,geometry.Bounds);
                AssertFeaturedInView(camera,geometry);orbit.Interest(out _,out double overview);Assert.That(overview,Is.Zero);
                camera.aspect=9f/16;orbit.ResizeIfNeeded();AssertFeaturedInView(camera,geometry);
                orbit.SetView(new Vector3(20,0.85f,30),12,75,20);Vector3 target=orbit.Target;float distance=orbit.Distance,yaw=orbit.Yaw,pitch=orbit.Pitch;
                camera.aspect=2.2f;orbit.ResizeIfNeeded();Assert.That(orbit.Target,Is.EqualTo(target));Assert.That(orbit.Distance,Is.EqualTo(distance));Assert.That(orbit.Yaw,Is.EqualTo(yaw));Assert.That(orbit.Pitch,Is.EqualTo(pitch));
                orbit.Reset();AssertFeaturedInView(camera,geometry);Assert.That(orbit.IsDefaultView,Is.True);
            }
            finally {Object.DestroyImmediate(go);}
        }
        [Test]
        public void CameraInterestUsesExplicitDistanceLatchAndBoundedHorizonFallbackAtTenHertz()
        {
            var go=new GameObject("Slice interest test");
            try
            {
                var camera=go.AddComponent<Camera>();camera.aspect=16f/9;var orbit=new SliceCamera(camera,Geometry().Bounds);
                Assert.That(orbit.TryInterest(0,out _,out double radius),Is.True);Assert.That(radius,Is.Zero);
                foreach(var state in new[] {new Vector2(200,1),new Vector2(240,1),new Vector2(250,0),new Vector2(240,0),new Vector2(225,1)})
                {
                    orbit.SetView(Vector3.up*0.85f,state.x,25,60);orbit.Interest(out _,out radius);
                    if(state.y==0) Assert.That(radius,Is.Zero);else Assert.That(radius,Is.InRange(20,80));
                }
                orbit.SetView(Vector3.up*0.85f,100,0,8);orbit.Interest(out _,out radius);Assert.That(radius,Is.EqualTo(25).Within(0.0001));
                orbit.Reset();int sent=0;
                for(int i=0;i<100;i++) {orbit.Orbit(new Vector2(0.1f,0));if(orbit.TryInterest(1+i*0.01,out _,out _)) sent++;}
                Assert.That(sent,Is.EqualTo(10));Assert.That(orbit.TryInterest(2,out var latest,out _),Is.True);Assert.That(latest,Is.EqualTo(orbit.Target));
            }
            finally {Object.DestroyImmediate(go);}
        }
        [Test]
        public void RepeatedOrbitPanZoomClampFiniteAndWarmInterestAllocatesNoManagedBytes()
        {
            var go=new GameObject("Slice bounded camera test");
            try
            {
                var camera=go.AddComponent<Camera>();camera.aspect=16f/9;var orbit=new SliceCamera(camera,Geometry().Bounds);
                for(int i=0;i<100;i++) {orbit.Orbit(new Vector2(10000,-10000));orbit.Pan(new Vector2(10000,-10000));orbit.Zoom(i%2==0?0.00001f:100000);}
                Assert.That(orbit.Distance,Is.InRange(4,1500));Assert.That(orbit.Yaw,Is.InRange(0,360));Assert.That(orbit.Pitch,Is.InRange(8,85));
                Assert.That(float.IsNaN(camera.transform.position.x)||float.IsInfinity(camera.transform.position.x),Is.False);
                Assert.Throws<ArgumentException>(()=>orbit.Zoom(float.NaN));Assert.Throws<ArgumentException>(()=>orbit.Pan(new Vector2(float.NaN,0)));
                orbit.SetView(Vector3.up*0.85f,100,25,60);for(int i=0;i<10;i++) orbit.Interest(out _,out _);
                long before=GC.GetAllocatedBytesForCurrentThread();for(int i=0;i<100;i++) orbit.Interest(out _,out _);
                Assert.That(GC.GetAllocatedBytesForCurrentThread()-before,Is.Zero);
            }
            finally {Object.DestroyImmediate(go);}
        }
        [UnityTest]
        public IEnumerator PlayCameraMovesFromOverviewToStreetAndBackWithoutLosingFeaturedFraming()
        {
            yield return new EnterPlayMode();
            var go=new GameObject("Slice play camera test");
            try
            {
                var camera=go.AddComponent<Camera>();camera.aspect=16f/9;var geometry=Geometry();var orbit=new SliceCamera(camera,geometry.Bounds);
                yield return null;orbit.Update(true);AssertFeaturedInView(camera,geometry);
                orbit.Zoom(0.01f);orbit.Pan(new Vector2(20,-15));orbit.Orbit(new Vector2(30,80));yield return null;
                Assert.That(orbit.Distance,Is.InRange(4,15));Assert.That(camera.nearClipPlane,Is.LessThan(0.1f));
                orbit.Zoom(1000);yield return null;Assert.That(orbit.Distance,Is.EqualTo(1500));orbit.Interest(out _,out double radius);Assert.That(radius,Is.Zero);
                camera.aspect=9f/16;orbit.Reset();yield return null;AssertFeaturedInView(camera,geometry);
            }
            finally {Object.DestroyImmediate(go);}
            yield return new ExitPlayMode();
        }
    }
}
