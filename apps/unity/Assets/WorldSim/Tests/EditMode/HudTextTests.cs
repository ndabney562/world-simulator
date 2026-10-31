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
        public void FeedLinesForNewTags()
        {
            Assert.That(Feed(14),Is.EqualTo("Maria Lopez was blessed"));
            Assert.That(Feed(15),Is.EqualTo("Maria Lopez feels refreshed"));
            Assert.That(Feed(16),Is.EqualTo("A storm rolls in"));
            Assert.That(Feed(17,b:3),Is.EqualTo("Maria Lopez stayed inside"));
            Assert.That(Feed(18,a:4),Is.EqualTo("The storm kept 4 people inside"));
            Assert.That(Feed(18,a:1),Is.EqualTo("The storm kept 1 person inside"));
            Assert.That(Feed(18,a:0),Is.EqualTo("The storm kept 0 people inside"));
            Assert.That(Feed(19),Is.EqualTo("The storm has passed"));
            Assert.That(Feed(20,a:3),Is.EqualTo("A gathering was called"));
            Assert.That(Feed(21,b:3),Is.EqualTo("Maria Lopez joined the gathering"));
            Assert.That(Feed(22,a:3,b:5),Is.EqualTo("The gathering ended (5 came)"));
            // A ReturnedHome in a gathering group is a walk home, not going home to rest.
            var home=new InspectionEvent {Tag=6,A=1,B=9};
            Assert.That(HudText.Feed(home,Name,20),Is.EqualTo("Maria Lopez headed home"));
            Assert.That(HudText.Feed(home,Name,13),Is.EqualTo("Maria Lopez went home to rest."));
        }
        static SliceConsequences Group(ushort root,params (uint tag,ulong a)[] rows)
        {
            var ev=new InspectionEvent[rows.Length];
            for(int i=0;i<rows.Length;i++) ev[i]=new InspectionEvent {Tag=rows[i].tag,A=rows[i].a};
            return new SliceConsequences {RootTag=root,Events=ev};
        }
        [Test]
        public void FinishedPerRootTag()
        {
            Assert.That(HudText.Finished(Group(14,(14,1))),Is.True,"A bless group is complete when committed.");
            Assert.That(HudText.Finished(Group(14,(14,1),(15,1))),Is.True);
            Assert.That(HudText.Finished(Group(16,(16,0))),Is.False);
            Assert.That(HudText.Finished(Group(16,(16,0),(18,3))),Is.False);
            Assert.That(HudText.Finished(Group(16,(16,0),(18,3),(19,0))),Is.True);
            Assert.That(HudText.Finished(Group(20,(20,9),(21,1),(21,2))),Is.False);
            var ended=Group(20,(20,9),(21,1),(21,2),(22,9));ended.Events[3].B=2;
            Assert.That(HudText.Finished(ended),Is.False,"Attendees are still on their way home.");
            Assert.That(HudText.Finished(Group(20,(20,9),(21,1),(21,2),(22,9),(6,1))),Is.False);
            Assert.That(HudText.Finished(Group(20,(20,9),(21,1),(21,2),(6,1),(6,2),(22,9))),Is.True);
            Assert.That(HudText.Finished(Group(20,(20,9),(22,9))),Is.True,"Nobody came, nobody to wait for.");
        }
        [Test]
        public void GatheringFinishesPerJoinerOrAtTheGraceCap()
        {
            const long end=1000000000;
            // A cursed joiner's ReturnedHome belongs to the illness root, so this group never gets it:
            // the watch finishes at the 30-minute cap after the end, not before.
            var interrupted=Group(20,(20,9),(21,1),(21,2),(22,9),(6,1));interrupted.End=end;
            interrupted.Time=end+HudText.GatheringGraceUs-100000;
            Assert.That(HudText.Finished(interrupted),Is.False,"Joiner 2 may still be walking home.");
            interrupted.Time=end+HudText.GatheringGraceUs;
            Assert.That(HudText.Finished(interrupted),Is.True,"The cap covers an interrupted joiner.");
            // A late non-joiner (never arrived before the end) also records ReturnedHome; its row
            // must not stand in for a joiner who is still on the way.
            var late=Group(20,(20,9),(21,1),(21,2),(22,9),(6,1),(6,3));late.End=end;late.Time=end;
            Assert.That(HudText.Finished(late),Is.False,"Two returns, but joiner 2 is not home.");
            var done=Group(20,(20,9),(21,1),(21,2),(22,9),(6,3),(6,1),(6,2));done.End=end;done.Time=end;
            Assert.That(HudText.Finished(done),Is.True);
            // Before the end nothing finishes, even past the joins.
            var early=Group(20,(20,9),(21,1),(6,1));early.End=end;early.Time=end+HudText.GatheringGraceUs;
            Assert.That(HudText.Finished(early),Is.False,"No GatheringEnded yet.");
        }
        [Test]
        public void CureEndedIllnessFinishesAtItsScheduledEnd()
        {
            // A bless cure keeps the illness group's scheduled end and adds no 12 to it.
            var ill=Group(13,(13,1),(5,1),(6,1));ill.Events[0].E=1;ill.End=7200000000;ill.Time=7200000000-100000;
            Assert.That(HudText.Finished(ill),Is.False);
            ill.Time=7200000000;
            Assert.That(HudText.Finished(ill),Is.True,"No row can follow the scheduled end.");
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
        [Test]
        public void EachCastWatchCarriesItsRootTag()
        {
            var watches=new System.Collections.Generic.List<HudWatch>();
            Assert.That(HudCasts.Try(watches,HudCasts.BlessTag,5,()=>101),Is.Null);
            Assert.That(HudCasts.Try(watches,HudCasts.StormTag,5,()=>102),Is.Null);
            HudCasts.Watch(watches,HudCasts.CurseTag,6,103);
            Assert.That(watches.Count,Is.EqualTo(3));
            Assert.That((watches[0].RootTag,watches[0].Root,watches[0].Subject),Is.EqualTo(((ushort)14,101UL,5UL)));
            Assert.That((watches[1].RootTag,watches[1].Root,watches[1].Subject),Is.EqualTo(((ushort)16,102UL,0UL)),"A storm has no subject.");
            Assert.That((watches[2].RootTag,watches[2].Root,watches[2].Subject),Is.EqualTo(((ushort)13,103UL,6UL)));
            watches[1].Done=true;
            Assert.That(HudCasts.Try(watches,HudCasts.GatherTag,7,()=>104),Is.Null);
            Assert.That((watches[3].RootTag,watches[3].Root,watches[3].Subject),Is.EqualTo(((ushort)20,104UL,7UL)));
        }
        [Test]
        public void RejectedCastAddsNoWatchAndNeverShowsBridgeText()
        {
            var watches=new System.Collections.Generic.List<HudWatch>();
            string warning=HudCasts.Try(watches,HudCasts.BlessTag,5,()=>throw new System.InvalidOperationException("Native slice status 7: slice operation stopped after committed prefix"));
            Assert.That(warning,Is.EqualTo("That didn't work. Try another target."));
            Assert.That(watches,Is.Empty);
            // An unfinished storm or gathering refuses both block powers before the bridge is called.
            HudCasts.Watch(watches,HudCasts.GatherTag,9,50);bool called=false;
            Assert.That(HudCasts.Try(watches,HudCasts.StormTag,0,()=>{called=true;return 51;}),Is.EqualTo("A storm or gathering is already under way."));
            Assert.That(HudCasts.Try(watches,HudCasts.GatherTag,9,()=>{called=true;return 52;}),Is.EqualTo(HudCasts.Busy));
            Assert.That(called,Is.False);Assert.That(watches.Count,Is.EqualTo(1));
            // Resident powers are not blocked by a block power.
            Assert.That(HudCasts.Try(watches,HudCasts.BlessTag,5,()=>53),Is.Null);Assert.That(watches.Count,Is.EqualTo(2));
        }
        static SliceGoal G(uint kind,uint target,uint progress=0,bool done=false)=>new SliceGoal {Kind=kind,Target=target,Progress=progress,Done=done};
        [Test]
        public void GoalAndFaithTextIsExact()
        {
            Assert.That(HudText.Faith(12),Is.EqualTo("Faith +12"));Assert.That(HudText.Faith(0),Is.EqualTo("Faith 0"));Assert.That(HudText.Faith(-7),Is.EqualTo("Faith -7"));
            Assert.That(HudText.Goal(G(1,3)),Is.EqualTo("Bring 3 people to a gathering"));
            Assert.That(HudText.Goal(G(2,2)),Is.EqualTo("Bless 2 different people"));
            Assert.That(HudText.Goal(G(3,1)),Is.EqualTo("Cure someone with a blessing"));
            Assert.That(HudText.Goal(G(4,4)),Is.EqualTo("Keep 4 people inside during a storm"));
            Assert.That(HudText.Goal(G(1,1)),Is.EqualTo("Bring 1 person to a gathering"));
            Assert.That(HudText.Goal(G(2,1)),Is.EqualTo("Bless 1 person"));
            Assert.That(HudText.Goal(G(4,1)),Is.EqualTo("Keep 1 person inside during a storm"));
            Assert.That(HudText.GoalLine(G(2,2,1)),Is.EqualTo("Bless 2 different people \u2014 1/2"));
            Assert.That(HudText.GoalLine(G(2,2,2,true)),Is.EqualTo("Done: Bless 2 different people"));
            Assert.That(HudText.GoalComplete(G(3,1,1,true)),Is.EqualTo("Goal complete: Cure someone with a blessing"));
        }
        static SliceGod Day(long day,params SliceGoal[] goals)=>new SliceGod {Day=day,Goals=goals};
        [Test]
        public void GoalWatchReportsOnlySameDayFlips()
        {
            var w=new GoalWatch();var got=new System.Collections.Generic.List<SliceGoal>();
            w.Update(Day(1,G(2,2,2,true),G(1,3)),got);Assert.That(got,Is.Empty,"first poll");
            w.Update(Day(1,G(2,2,2,true),G(1,3,3,true)),got);Assert.That(got.Count,Is.EqualTo(1));Assert.That(got[0].Kind,Is.EqualTo(1u));
            got.Clear();w.Update(Day(1,G(2,2,2,true),G(1,3,3,true)),got);Assert.That(got,Is.Empty,"repeat poll");
            w.Update(Day(2,G(2,2),G(1,3),G(4,2)),got);Assert.That(got,Is.Empty,"day change");
            w.Update(Day(2,G(2,2),G(1,3),G(4,2,2,true)),got);Assert.That(got.Count,Is.EqualTo(1));
            got.Clear();w.Reset();w.Update(Day(2,G(2,2,2,true),G(1,3),G(4,2,2,true)),got);Assert.That(got,Is.Empty,"after reset");
        }
        [Test]
        public void GoalWatchClearsStaleFlagsWhenGoalCountShrinks()
        {
            var w=new GoalWatch();var got=new System.Collections.Generic.List<SliceGoal>();
            w.Update(Day(1,G(2,2),G(1,3),G(4,2,2,true)),got);
            w.Update(Day(2,G(2,2)),got);
            w.Update(Day(2,G(2,2),G(1,3),G(4,2,2,true)),got);
            Assert.That(got.Count,Is.EqualTo(1),"slot 2 is a new completion, not a stale flag");
        }
        [Test]
        public void FaithWordBoundaries()
        {
            Assert.That(HudText.FaithWord(40),Is.EqualTo("devoted"));Assert.That(HudText.FaithWord(39),Is.EqualTo("trusting"));
            Assert.That(HudText.FaithWord(10),Is.EqualTo("trusting"));Assert.That(HudText.FaithWord(9),Is.EqualTo("unsure"));
            Assert.That(HudText.FaithWord(-9),Is.EqualTo("unsure"));Assert.That(HudText.FaithWord(-10),Is.EqualTo("wary"));
            Assert.That(HudText.FaithWord(-39),Is.EqualTo("wary"));Assert.That(HudText.FaithWord(-40),Is.EqualTo("fearful"));
            Assert.That(HudText.CardFaith(15),Is.EqualTo("Faith: trusting"));
        }
        [UnityTest]
        public IEnumerator BlessUpdatesMeterAndCard()
        {
            yield return new EnterPlayMode();
            var loading=EditorSceneManager.LoadSceneAsyncInPlayMode("Assets/WorldSim/Scenes/RealBlock.unity",new LoadSceneParameters(LoadSceneMode.Single));
            while(!loading.isDone) yield return null;
            var demo=Object.FindFirstObjectByType<SliceDemo>();var hud=Object.FindFirstObjectByType<GameHud>();
            Assert.That(hud,Is.Not.Null);
            for(int i=0;i<3;i++) yield return null;
            demo.SetSpeed(1);demo.RequestStreetView();double deadline=Time.realtimeSinceStartupAsDouble+8;
            while(demo.WaitingForStreet&&Time.realtimeSinceStartupAsDouble<deadline) yield return null;
            demo.SetSpeed(0);yield return null;
            var mesh=ToyMeshes.Capsule();var picker=new SlicePicker(demo.Geometry,mesh);
            hud.ToggleBless();bool cast=false;
            for(int i=0;i<demo.CurrentFrame.RenderedResidents&&!cast;i++)
            {
                Vector3 point=demo.SceneCamera.WorldToScreenPoint(demo.CurrentFrame.ResidentMatrices[i].MultiplyPoint3x4(Vector3.zero));
                if(point.z<=0||!picker.TryPick(demo.SceneCamera.ScreenPointToRay(point),demo.CurrentFrame,out var hit)||!hit.Resident) continue;
                if(demo.InspectScreenPoint(point)) cast=true;
            }
            Object.DestroyImmediate(mesh);
            Assert.That(cast,Is.True,"A visible resident must be tappable.");
            double until=Time.realtimeSinceStartupAsDouble+2;
            while(Time.realtimeSinceStartupAsDouble<until&&hud.CardFaithText!="Faith: trusting") yield return null;
            Assert.That(hud.CardFaithText,Is.EqualTo("Faith: trusting"));
            Assert.That(hud.MeterText,Is.EqualTo(HudText.Faith(demo.God().BlockFaith)));
            yield return new ExitPlayMode();
        }
        [UnityTest]
        public IEnumerator SecondStormIsRefusedAndKeepsThePowerArmed()
        {
            yield return new EnterPlayMode();
            var loading=EditorSceneManager.LoadSceneAsyncInPlayMode("Assets/WorldSim/Scenes/RealBlock.unity",new LoadSceneParameters(LoadSceneMode.Single));
            while(!loading.isDone) yield return null;
            var hud=Object.FindFirstObjectByType<GameHud>();Assert.That(hud,Is.Not.Null);
            for(int i=0;i<3;i++) yield return null;
            hud.CastStorm();
            Assert.That(hud.Watches.Count,Is.EqualTo(1));Assert.That(hud.Watches[0].RootTag,Is.EqualTo(16));Assert.That(hud.Warning,Is.Empty);
            hud.ToggleBless();Assert.That(hud.PowerArmed,Is.True);
            hud.CastStorm();
            Assert.That(hud.Watches.Count,Is.EqualTo(1),"A refused cast creates no watch.");
            Assert.That(hud.Warning,Is.EqualTo("A storm or gathering is already under way."));
            Assert.That(hud.PowerArmed,Is.True,"A failed cast keeps the armed power.");
            yield return new ExitPlayMode();
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
