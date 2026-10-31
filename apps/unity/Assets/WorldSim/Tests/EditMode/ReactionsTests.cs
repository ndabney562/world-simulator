using System;
using System.Collections;
using NUnit.Framework;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.SceneManagement;
using UnityEngine.TestTools;
using WorldSim.Bridge;
using WorldSim.Rendering;
using WorldSim.UI;
using Object=UnityEngine.Object;

namespace WorldSim.Tests.EditMode
{
    public sealed class ReactionsTests
    {
        static InspectionEvent Row(uint tag,ulong a=1,ulong b=0,ulong c=0,ulong d=0,ulong e=0,long time=0)=>new InspectionEvent {Tag=tag,A=a,B=b,C=c,D=d,E=e,Time=time};
        const long Hour=3600000000;
        [Test]
        public void IllnessShowsSickUntilRecoveryThenRecoveringForAnHour()
        {
            var s=new ResidentStatus();
            s.Apply(Row(13,e:1),13,10*Hour);
            Assert.That(s.Get(1,Hour),Is.EqualTo(StatusIcon.Sick));
            Assert.That(s.Get(1,10*Hour),Is.EqualTo(StatusIcon.None),"An illness without a recovery row ends at its scheduled end.");
            s.Apply(Row(12,time:2*Hour),13,10*Hour);
            Assert.That(s.Get(1,2*Hour+1),Is.EqualTo(StatusIcon.Recovering));
            Assert.That(s.Get(1,3*Hour),Is.EqualTo(StatusIcon.None));
        }
        [Test]
        public void BlessCureMovesSickToRecovering()
        {
            var s=new ResidentStatus();
            s.Apply(Row(13,e:1),13,10*Hour);
            // A bless cure puts the Recovered row in the bless group (ADR 0038 §2).
            s.Apply(Row(14,c:1,time:Hour),14,0);s.Apply(Row(12,time:Hour),14,0);
            Assert.That(s.Get(1,Hour+1),Is.EqualTo(StatusIcon.Recovering));
        }
        [Test]
        public void NoEffectCurseShowsNothingAndNeighbourVisitLastsItsDuration()
        {
            var s=new ResidentStatus();
            s.Apply(Row(13,e:0),13,10*Hour);Assert.That(s.Get(1,0),Is.EqualTo(StatusIcon.None));
            s.Apply(Row(11,a:2,b:1,c:9,d:300000000,time:Hour),13,10*Hour);
            Assert.That(s.Get(2,Hour+1,out ulong building),Is.EqualTo(StatusIcon.Visiting));Assert.That(building,Is.EqualTo(9));
            Assert.That(s.Get(2,Hour+300000000),Is.EqualTo(StatusIcon.None));
        }
        [Test]
        public void StormSheltersUntilItPassesAndSickWinsOverOtherStatuses()
        {
            var s=new ResidentStatus();
            s.BeginStorm(Hour);Assert.That(s.StormActive(0),Is.True);
            s.Apply(Row(16,a:1,b:Hour),16,2*Hour);Assert.That(s.StormUntil,Is.EqualTo(2*Hour));
            s.Apply(Row(17,a:3,b:7),16,2*Hour);
            Assert.That(s.Get(3,Hour,out ulong building),Is.EqualTo(StatusIcon.Sheltering));Assert.That(building,Is.EqualTo(7));
            s.Apply(Row(13,a:3,e:1),13,5*Hour);Assert.That(s.Get(3,Hour),Is.EqualTo(StatusIcon.Sick));
            s.Apply(Row(19),16,2*Hour);
            Assert.That(s.StormActive(Hour),Is.False);Assert.That(s.Get(3,Hour),Is.EqualTo(StatusIcon.Sick));
            s.Apply(Row(12,a:3,time:Hour),13,5*Hour);Assert.That(s.Get(3,Hour+1),Is.EqualTo(StatusIcon.Recovering),"The storm's end cleared sheltering.");
        }
        [Test]
        public void GatheringSocialisesUntilTheJoinerHeadsHome()
        {
            var s=new ResidentStatus();
            s.Apply(Row(21,a:4,b:8),20,Hour);
            Assert.That(s.Get(4,Hour+1,out ulong building),Is.EqualTo(StatusIcon.Socialising));Assert.That(building,Is.EqualTo(8));
            s.Apply(Row(6,a:4,b:8),13,Hour);Assert.That(s.Get(4,0),Is.EqualTo(StatusIcon.Socialising),"An illness return home is not the gathering's.");
            s.Apply(Row(6,a:4,b:8),20,Hour);Assert.That(s.Get(4,0),Is.EqualTo(StatusIcon.None));
            s.Apply(Row(21,a:5,b:8),20,Hour);
            Assert.That(s.Get(5,Hour+HudText.GatheringGraceUs),Is.EqualTo(StatusIcon.None),"A joiner whose return went to another root stops at the grace cap.");
            s.Clear();Assert.That(s.Get(5,0),Is.EqualTo(StatusIcon.None));
        }
        [Test]
        public void EffectShapesGrowThenVanish()
        {
            foreach(EffectKind kind in Enum.GetValues(typeof(EffectKind)))
            {
                double d=ReactionMath.Duration(kind);
                Assert.That(ReactionMath.Size(kind,0),Is.Zero);Assert.That(ReactionMath.Size(kind,d*0.25),Is.EqualTo(1).Within(1e-5));
                Assert.That(ReactionMath.Size(kind,d),Is.Zero);Assert.That(ReactionMath.Alive(kind,d),Is.False);Assert.That(ReactionMath.Alive(kind,-0.01),Is.False);
            }
            Assert.That(ReactionMath.RingRadius(0),Is.Zero);
            Assert.That(ReactionMath.RingRadius(1),Is.LessThan(ReactionMath.RingRadius(2)));
            Assert.That(ReactionMath.RingRadius(2),Is.LessThanOrEqualTo(ReactionMath.GatheringRadius));
            Assert.That(ReactionMath.Ease(0),Is.Zero);Assert.That(ReactionMath.Ease(ReactionMath.EaseSeconds*0.5),Is.EqualTo(0.5f).Within(1e-5));
            Assert.That(ReactionMath.Ease(ReactionMath.EaseSeconds),Is.EqualTo(1));Assert.That(ReactionMath.Ease(9),Is.EqualTo(1));
            for(double t=0;t<5;t+=0.37) foreach(float phase in new[]{0f,0.5f,0.999f})
                Assert.That(ReactionMath.RainY(t,phase),Is.InRange(0f,ReactionMath.RainHeight));
            Assert.That(Mathf.Abs(ReactionMath.Bob(0.123)),Is.LessThanOrEqualTo(0.15f));
        }
        [Test]
        public void CameraEasesToTargetAndTrackKeepsZoomAndAngle()
        {
            var go=new GameObject("Ease test");
            try
            {
                var camera=go.AddComponent<Camera>();camera.aspect=16f/9;
                var orbit=new SliceCamera(camera,new Bounds(Vector3.zero,new Vector3(80,20,80)));
                orbit.SetView(new Vector3(0,0.85f,0),200,30,50);
                orbit.EaseTo(new Vector3(20,0.85f,-10),60,10);Assert.That(orbit.Easing,Is.True);
                orbit.StepEase(10+ReactionMath.EaseSeconds*0.5,ReactionMath.Ease,ReactionMath.EaseSeconds);
                Assert.That(orbit.Target.x,Is.EqualTo(10).Within(1e-3));Assert.That(orbit.Distance,Is.EqualTo(130).Within(1e-3));Assert.That(orbit.Easing,Is.True);
                orbit.StepEase(10+ReactionMath.EaseSeconds,ReactionMath.Ease,ReactionMath.EaseSeconds);
                Assert.That(orbit.Target.x,Is.EqualTo(20).Within(1e-4));Assert.That(orbit.Target.z,Is.EqualTo(-10).Within(1e-4));Assert.That(orbit.Distance,Is.EqualTo(60).Within(1e-4));
                Assert.That(orbit.Easing,Is.False);Assert.That(orbit.Yaw,Is.EqualTo(30));Assert.That(orbit.Pitch,Is.EqualTo(50));
                uint before=orbit.Revision;orbit.EaseTo(new Vector3(0,0.85f,0),60,20);orbit.RetargetEase(new Vector3(-8,0.85f,4));
                Assert.That(orbit.Revision,Is.EqualTo(before),"Starting or retargeting an ease does not move the camera yet.");
                orbit.StepEase(20+ReactionMath.EaseSeconds,ReactionMath.Ease,ReactionMath.EaseSeconds);
                Assert.That(orbit.Target.x,Is.EqualTo(-8).Within(1e-4));Assert.That(orbit.Revision,Is.Not.EqualTo(before));
                orbit.EaseTo(Vector3.zero,60,30);orbit.CancelEase();Assert.That(orbit.Easing,Is.False);
                orbit.Track(new Vector3(5,3,7));
                Assert.That(orbit.Target,Is.EqualTo(new Vector3(5,0.85f,7)));Assert.That(orbit.Distance,Is.EqualTo(60).Within(1e-4));Assert.That(orbit.Yaw,Is.EqualTo(30));
            }
            finally {Object.DestroyImmediate(go);}
        }
        [UnityTest]
        public IEnumerator CastsShowEffectsIconsAndStormAndFollowStopsWhenCardCloses()
        {
            yield return new EnterPlayMode();
            var loading=EditorSceneManager.LoadSceneAsyncInPlayMode("Assets/WorldSim/Scenes/RealBlock.unity",new LoadSceneParameters(LoadSceneMode.Single));
            while(!loading.isDone) yield return null;
            var demo=Object.FindFirstObjectByType<SliceDemo>();var hud=Object.FindFirstObjectByType<GameHud>();var reactions=Object.FindFirstObjectByType<Reactions>();
            Assert.That(reactions,Is.Not.Null,"The RealBlock scene carries the reactions component.");
            for(int i=0;i<3;i++) yield return null;
            demo.SetSpeed(1);demo.RequestStreetView();double deadline=Time.realtimeSinceStartupAsDouble+8;
            while(demo.WaitingForStreet&&Time.realtimeSinceStartupAsDouble<deadline) yield return null;
            Assert.That(demo.WaitingForStreet,Is.False);demo.SetSpeed(0);yield return null;
            // Curse a visible resident through the HUD, as a player would.
            var mesh=ToyMeshes.Capsule();var picker=new SlicePicker(demo.Geometry,mesh);
            hud.TogglePower();bool cast=false;
            for(int i=0;i<demo.CurrentFrame.RenderedResidents&&!cast;i++)
            {
                Vector3 point=demo.SceneCamera.WorldToScreenPoint(demo.CurrentFrame.ResidentMatrices[i].MultiplyPoint3x4(Vector3.zero));
                if(point.z<=0||!picker.TryPick(demo.SceneCamera.ScreenPointToRay(point),demo.CurrentFrame,out var hit)||!hit.Resident) continue;
                if(demo.InspectScreenPoint(point)) cast=true;
            }
            Object.DestroyImmediate(mesh);
            Assert.That(cast,Is.True,"A visible resident must be tappable.");
            Assert.That(reactions.ActiveEffects,Is.EqualTo(1),"The curse shows where it lands.");
            Assert.That(demo.Orbit.Easing,Is.True,"The camera eases to the target.");
            // Warmed drawing allocates nothing, with a live effect and advancing time.
            double start=Time.realtimeSinceStartupAsDouble;
            for(int i=0;i<10;i++) reactions.Draw(start+i*0.01);
            long allocation=GC.GetAllocatedBytesForCurrentThread();
            for(int i=10;i<20;i++) reactions.Draw(start+i*0.01);
            Assert.That(GC.GetAllocatedBytesForCurrentThread()-allocation,Is.Zero,"Warmed reactions must reuse matrices and materials.");
            Assert.That(reactions.ActiveEffects,Is.EqualTo(1));
            ulong patient=demo.CurrentInspection.Id;
            for(int i=0;i<3;i++) yield return null;
            Assert.That(reactions.Status.Get(patient,demo.CurrentFrame.SimTimeMicroseconds),Is.EqualTo(StatusIcon.Sick));
            Assert.That(reactions.DrawnIcons,Is.GreaterThanOrEqualTo(1));
            // Follow, then Overview hands the camera back: no pull to the resident, the default fit holds.
            hud.ToggleFollow();Assert.That(reactions.Following,Is.EqualTo(patient));
            double until=Time.realtimeSinceStartupAsDouble+1;
            while(Time.realtimeSinceStartupAsDouble<until) yield return null;
            Assert.That(reactions.Following,Is.EqualTo(patient),"Following holds while nothing else moves the camera.");
            demo.ResetView();
            until=Time.realtimeSinceStartupAsDouble+0.5;
            while(Time.realtimeSinceStartupAsDouble<until) yield return null;
            Assert.That(reactions.Following,Is.Zero);Assert.That(demo.Orbit.IsDefaultView,Is.True);Assert.That(demo.Orbit.Easing,Is.False);
            // Following again, then closing the card stops it.
            hud.ToggleFollow();Assert.That(reactions.Following,Is.EqualTo(patient));
            yield return null;demo.ClearInspection();
            // Editor updates outpace player frames at 60 fps: wait real time, not a frame count.
            until=Time.realtimeSinceStartupAsDouble+1;
            while(reactions.Following!=0&&Time.realtimeSinceStartupAsDouble<until) yield return null;
            Assert.That(reactions.Following,Is.Zero);
            hud.CastStorm();until=Time.realtimeSinceStartupAsDouble+1;
            while(!reactions.StormVisible&&Time.realtimeSinceStartupAsDouble<until) yield return null;
            Assert.That(reactions.StormVisible,Is.True,"The storm cloud shows at once.");
            yield return new ExitPlayMode();
        }
    }
}
