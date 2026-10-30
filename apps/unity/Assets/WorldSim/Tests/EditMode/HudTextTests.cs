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
    public sealed class HudTextTests
    {
        static string Name(ulong id)=>id==1?"Maria Lopez":id==2?"Ahmed Khan":"Someone";
        static string Feed(uint tag,ulong a=1,ulong b=0,ulong c=0,ulong e=0)=>HudText.Feed(new InspectionEvent {Tag=tag,A=a,B=b,C=c,E=e},Name);
        [Test]
        public void FeedTextCoversEveryConsequenceTag()
        {
            Assert.That(Feed(13,e:1),Is.EqualTo("You cursed Maria Lopez with illness."));
            Assert.That(Feed(13,e:0),Is.EqualTo("Maria Lopez is already unwell; the curse had no effect."));
            Assert.That(Feed(5),Is.EqualTo("Maria Lopez fell sick."));
            Assert.That(Feed(10,b:7,c:2),Is.EqualTo("Maria Lopez is sick and missed work."));
            Assert.That(Feed(10,b:7,c:3),Is.EqualTo("Maria Lopez is sick and missed school."));
            Assert.That(Feed(11,a:2,b:1,c:9),Is.EqualTo("Ahmed Khan checked on their neighbour Maria Lopez."));
            Assert.That(Feed(6,b:9),Is.EqualTo("Maria Lopez went home to rest."));
            Assert.That(Feed(12,b:100),Is.EqualTo("Maria Lopez recovered."));
        }
        [Test]
        public void FeedIgnoresEventsOutsideTheConsequenceVocabulary()
        {
            Assert.That(Feed(7),Is.Null);Assert.That(Feed(9),Is.Null);Assert.That(Feed(99),Is.Null);
        }
        [Test]
        public void ChainFinishesOnRecoveryOrANoEffectCurse()
        {
            var running=new SliceConsequences {Events=new[]{new InspectionEvent {Tag=13,E=1},new InspectionEvent {Tag=5}}};
            Assert.That(HudText.Finished(running),Is.False);
            Assert.That(HudText.Finished(new SliceConsequences {Events=new[]{new InspectionEvent {Tag=13,E=0}}}),Is.True);
            Assert.That(HudText.Finished(new SliceConsequences {Events=new[]{new InspectionEvent {Tag=13,E=1},new InspectionEvent {Tag=12}}}),Is.True);
        }
        [Test]
        public void ResidentCardShowsJobHealthActivityAndTodaysSchedule()
        {
            const long day=86400000000;
            var r=new SliceInspection {GivenName="Maria",FamilyName="Lopez",Age=34,Role=2,Health=55,IllnessActive=true,Activity=5,ScheduleEpoch=0,
                Schedule=new[]{new InspectionSchedule {Table=0,Minute=0,Activity=0},new InspectionSchedule {Table=0,Minute=450,Activity=1},
                    new InspectionSchedule {Table=1,Minute=0,Activity=0},new InspectionSchedule {Table=1,Minute=600,Activity=4}}};
            Assert.That(HudText.Name(r),Is.EqualTo("Maria Lopez"));
            Assert.That(HudText.Resident(r),Is.EqualTo("Age 34 · Employed\nHealth 55/100 · Sick\nNow: At home"));
            r.Time=2*day+1;Assert.That(HudText.TodaySchedule(r),Is.EqualTo(new[]{"00:00 Sleeping","07:30 Commuting"}));
            r.Time=5*day;Assert.That(HudText.TodaySchedule(r),Is.EqualTo(new[]{"00:00 Sleeping","10:00 Running errands"}));
            r.Time=-1;Assert.That(HudText.TodaySchedule(r),Is.EqualTo(new[]{"00:00 Sleeping","10:00 Running errands"}),"Day -1 is model Sunday.");
        }
        [UnityTest]
        public IEnumerator SelectedPowerCastsOnTappedResidentAndFeedReportsIt()
        {
            yield return new EnterPlayMode();
            var loading=EditorSceneManager.LoadSceneAsyncInPlayMode("Assets/WorldSim/Scenes/RealBlock.unity",new LoadSceneParameters(LoadSceneMode.Single));
            while(!loading.isDone) yield return null;
            var demo=Object.FindFirstObjectByType<SliceDemo>();var hud=Object.FindFirstObjectByType<GameHud>();
            Assert.That(demo,Is.Not.Null);Assert.That(hud,Is.Not.Null,"The RealBlock scene carries the game HUD.");
            for(int i=0;i<3;i++) yield return null;
            Assert.That(demo.UiHitTest,Is.Not.Null);
            Assert.That(demo.UiHitTest(new Vector2(Screen.width*0.5f,30*SliceDemo.HudUnits)),Is.True,"The power bar captures taps.");
            Assert.That(demo.UiHitTest(new Vector2(Screen.width*0.5f,Screen.height*0.4f)),Is.False,"The open viewport stays tappable.");
            demo.SetSpeed(1);demo.RequestStreetView();double deadline=Time.realtimeSinceStartupAsDouble+8;
            while(demo.WaitingForStreet&&Time.realtimeSinceStartupAsDouble<deadline) yield return null;
            Assert.That(demo.WaitingForStreet,Is.False);demo.SetSpeed(0);yield return null;
            var mesh=ToyMeshes.Capsule();var picker=new SlicePicker(demo.Geometry,mesh);
            hud.TogglePower();Assert.That(hud.PowerArmed,Is.True);bool cast=false;
            for(int i=0;i<demo.CurrentFrame.RenderedResidents&&!cast;i++)
            {
                Vector3 point=demo.SceneCamera.WorldToScreenPoint(demo.CurrentFrame.ResidentMatrices[i].MultiplyPoint3x4(Vector3.zero));
                if(point.z<=0||!picker.TryPick(demo.SceneCamera.ScreenPointToRay(point),demo.CurrentFrame,out var hit)||!hit.Resident) continue;
                if(demo.InspectScreenPoint(point)) cast=true;
            }
            Object.DestroyImmediate(mesh);
            Assert.That(cast,Is.True,"A visible resident must be tappable.");
            Assert.That(hud.PowerArmed,Is.False,"Casting uses up the selection.");
            Assert.That(demo.CurrentInspection.Health,Is.EqualTo(55));
            for(int i=0;i<3;i++) yield return null;
            string name=HudText.Name(demo.CurrentInspection);
            Assert.That(hud.Feed,Has.Member(name+" fell sick."));Assert.That(hud.Feed,Has.Member("You cursed "+name+" with illness."));
            yield return new ExitPlayMode();
        }
    }
}
