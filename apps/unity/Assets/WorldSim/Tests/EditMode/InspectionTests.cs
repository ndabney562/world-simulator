using System;
using System.IO;
using System.Collections;
using UnityEditor.SceneManagement;
using UnityEngine.SceneManagement;
using UnityEngine.TestTools;
using NUnit.Framework;
using Unity.Collections;
using UnityEngine;
using WorldSim.Bridge;
using WorldSim.Rendering;
using Object=UnityEngine.Object;

namespace WorldSim.Tests.EditMode
{
    public sealed class InspectionTests
    {
        static long Start=>NativeWorld.UtcToSim(2026,6,22,11,22,30);
        static byte[] Fixture(string name)=>ConvertHex(File.ReadAllText(Path.Combine(Application.dataPath,"../../../core/crates/ws-ffi/tests/fixtures/"+name+".hex")));
        static byte[] ConvertHex(string hex) {hex=hex.Replace("\n","").Replace("\r","").Replace(" ","");var b=new byte[hex.Length/2];for(int i=0;i<b.Length;i++) b[i]=Convert.ToByte(hex.Substring(i*2,2),16);return b;}
        static ulong Id(byte[] b)=>BitConverter.ToUInt64(b,40);
        static void U32(byte[] b,int at,uint value)=>Array.Copy(BitConverter.GetBytes(value),0,b,at,4);
        static void U64(byte[] b,int at,ulong value)=>Array.Copy(BitConverter.GetBytes(value),0,b,at,8);
        static SliceGeometry Geometry() {using(var b=new NativeArray<byte>(Fixture("slice-geometry-v11"),Allocator.Temp)) return SliceDecoder.DecodeGeometry(b,b.Length);}
        [Test]
        public void ActualIllnessActionReadsHealthRecoveryAndRefreshNeverReapplies()
        {
            using(var world=new NativeSlice(71449,Start)) {
                var frame=new SliceFrame();world.Fill(frame,out _);ulong id=frame.Residents[0].Id;
                var original=world.Inspect(id);var sample=world.Illness(id,0,5,200000);
                Assert.That(sample.Inspection.Health,Is.EqualTo(55));Assert.That(sample.Inspection.Life,Is.Zero);
                Assert.That(sample.Inspection.Sex,Is.EqualTo(original.Sex));Assert.That(sample.Inspection.IllnessActive,Is.True);
                Assert.That(sample.Consequences.Events[0].Tag,Is.EqualTo(13));Assert.That(sample.Consequences.Events[1].Tag,Is.EqualTo(5));
                ulong root=sample.Consequences.Root,hash=sample.Inspection.StateHash;
                var refreshed=world.Illness(id,root);Assert.That(refreshed.Inspection.StateHash,Is.EqualTo(hash));Assert.That(refreshed.Consequences.Events.Length,Is.EqualTo(sample.Consequences.Events.Length));
                world.Steps(2);var recovered=world.Illness(id,root);Assert.That(recovered.Inspection.Health,Is.EqualTo(100));Assert.That(recovered.Inspection.IllnessActive,Is.False);
                Assert.That(recovered.Consequences.Events[recovered.Consequences.Events.Length-1].Tag,Is.EqualTo(12));
                StringAssert.Contains("Recovered",SliceInspectionText.Consequences(recovered));StringAssert.Contains("Direct",SliceInspectionText.Consequences(recovered));StringAssert.Contains("← Illness",SliceInspectionText.Consequences(recovered));
                Assert.That(SliceInspectionText.Format(recovered.Inspection).Split('\n').Length,Is.EqualTo(5));
            }
        }
        [Test]
        public void ActualNativeL1NeighbourVisitHasContributingProvenanceAndHeldSocialAction()
        {
            using(var world=new NativeSlice(42,NativeWorld.UtcToSim(2026,6,23,0,0,0))) {
                var frame=new SliceFrame();world.Fill(frame,out _);SliceInspection patient=null;
                for(int i=0;i<frame.Population&&patient==null;i++) {
                    var a=world.Inspect(frame.Residents[i].Id);
                    for(int j=i+1;j<frame.Population;j++) {
                        var b=world.Inspect(frame.Residents[j].Id);
                        if(a.Home==b.Home&&a.Household!=b.Household) {patient=a;break;}
                    }
                }
                Assert.That(patient,Is.Not.Null);
                foreach(var building in world.Geometry.Buildings) if(building.Id==patient.Home) world.SetCamera(new Vector3(building.Centroid.x,0,building.Centroid.y),50);
                world.Steps(20);Assert.That(world.Inspect(patient.Id).Detail,Is.EqualTo(1));
                var applied=world.Illness(patient.Id,0,5,600000000);
                for(int i=0;i<6;i++) world.Steps(100);
                var sample=world.Illness(patient.Id,applied.Consequences.Root);bool found=false;
                for(int i=0;i<sample.Consequences.Events.Length;i++) if(sample.Consequences.Events[i].Tag==11) {
                    var e=sample.Consequences.Events[i];Assert.That(world.Inspect(e.A).Action,Is.EqualTo(3));
                    Assert.That(sample.Consequences.Causes[i][0].Relation,Is.EqualTo(3));Assert.That(sample.Consequences.Causes[i][0].Confidence,Is.EqualTo(1));found=true;
                }
                Assert.That(found,Is.True);StringAssert.Contains("Neighbour check-in · Contributing",SliceInspectionText.Consequences(sample));
            }
        }
        [Test]
        public void ConsequencesRejectMalformedMetadataVocabularyCausesAndLengthsBeforePublication()
        {
            var geometry=Geometry();byte[] raw=Fixture("consequences-v11");ulong root=BitConverter.ToUInt64(raw,40);SliceConsequences published;
            using(var b=new NativeArray<byte>(raw,Allocator.Temp)) published=ConsequencesDecoder.Decode(b,b.Length,geometry,root);
            Assert.That(published.Events.Length,Is.GreaterThanOrEqualTo(2));
            foreach(Action<byte[]> corrupt in new Action<byte[]>[] {
                b=>U32(b,4,95),b=>U64(b,32,0),b=>U64(b,40,0),b=>U64(b,48,0),b=>U32(b,56,17),b=>U32(b,60,17),b=>U32(b,64,127),b=>U32(b,68,1),b=>U32(b,72,2),b=>U32(b,76,0),
                b=>U32(b,96+32,99),b=>U32(b,96+80,1),b=>U32(b,96+84,1),b=>U64(b,96+120,1),
                b=>U64(b,96+128,root),b=>U32(b,96+128+36,2),b=>U64(b,96+128+56,4),b=>U32(b,96+128+80,3),b=>U64(b,96+128+88,ulong.MaxValue),b=>U32(b,96+128+96,3),b=>U32(b,96+128+100,1),b=>U64(b,96+128+104,root)
            }) {
                byte[] copy=(byte[])raw.Clone();corrupt(copy);using(var b=new NativeArray<byte>(copy,Allocator.Temp)) Assert.Throws<ArgumentException>(()=>ConsequencesDecoder.Decode(b,b.Length,geometry,root));
            }
            using(var b=new NativeArray<byte>(raw,Allocator.Temp)) {
                Assert.Throws<ArgumentException>(()=>ConsequencesDecoder.Decode(b,b.Length-1,geometry,root));
                Assert.Throws<ArgumentException>(()=>ConsequencesDecoder.Decode(b,b.Length,geometry,root+1));
            }
            byte[] missing=new byte[96+128];Array.Copy(raw,missing,missing.Length);U32(missing,56,1);U32(missing,60,1);
            using(var b=new NativeArray<byte>(missing,Allocator.Temp)) Assert.Throws<ArgumentException>(()=>ConsequencesDecoder.Decode(b,b.Length,geometry,root));
            byte[] old=(byte[])raw.Clone();U32(old,0,8);using(var b=new NativeArray<byte>(old,Allocator.Temp)) Assert.Throws<InvalidOperationException>(()=>ConsequencesDecoder.Decode(b,b.Length,geometry,root));
            Assert.That(published.Root,Is.EqualTo(root));
            byte[] ill=Fixture("inspect-ill-v11");int state=96+(int)BitConverter.ToUInt32(ill,64);
            using(var b=new NativeArray<byte>(ill,Allocator.Temp)) Assert.That(InspectionDecoder.Decode(b,b.Length,geometry,Id(ill)).IllnessActive,Is.True);
            foreach(Action<byte[]> corrupt in new Action<byte[]>[] {
                b=>U32(b,state+8,6),b=>U64(b,state+32,ulong.MaxValue),b=>U32(b,state+80,3),b=>U32(b,state+84,2),b=>U64(b,state+88,0),b=>U64(b,state+96,0),b=>U64(b,state+104,0),b=>U32(b,state+56,100),b=>U32(b,state+60,1)
            }) {byte[] copy=(byte[])ill.Clone();corrupt(copy);using(var b=new NativeArray<byte>(copy,Allocator.Temp)) Assert.Throws<ArgumentException>(()=>InspectionDecoder.Decode(b,b.Length,geometry,Id(ill)));}
        }
        [Test]
        public void IllnessConsequencesWithoutRecoveryDecode()
        {
            // ADR0038 §2 / Review Focus4: a bless cure owns the Recovered row, so the
            // illness group keeps its scheduled end and has no recovery row.
            using(var world=new NativeSlice(71449,Start)) {
                var frame=new SliceFrame();world.Fill(frame,out _);ulong id=frame.Residents[0].Id;
                var ill=world.Illness(id,0,5,600000000);ulong illness=ill.Consequences.Root;long end=ill.Consequences.End;
                world.Steps(50);ulong bless=world.Bless(id);
                for(int i=0;i<61;i++) world.Steps(100);
                var sample=world.Illness(id,illness);
                Assert.That(sample.Consequences.RootTag,Is.EqualTo(13));Assert.That(sample.Consequences.End,Is.EqualTo(end));
                Assert.That(sample.Consequences.Time,Is.GreaterThan(end));
                foreach(var e in sample.Consequences.Events) Assert.That(e.Tag,Is.Not.EqualTo(12));
                Assert.That(sample.Inspection.Health,Is.EqualTo(100));Assert.That(sample.Inspection.IllnessActive,Is.False);Assert.That(sample.Inspection.OwnRoot,Is.EqualTo(illness));
                var blessed=world.Consequences(bless);Assert.That(blessed.Events[blessed.Events.Length-1].Tag,Is.EqualTo(12));
                int recovered=0;foreach(var e in world.Consequences(bless).Events) if(e.Tag==12) recovered++;Assert.That(recovered,Is.EqualTo(1));
                Assert.That(SliceInspectionText.Consequences(sample),Does.Not.Contain("Recovered"));
            }
        }
        [Test]
        public void RejectsUnknownRootTag()
        {
            var geometry=Geometry();
            foreach(string name in new[]{"consequences-v11","consequences-bless-v11","consequences-storm-v11","consequences-gathering-v11"})
            {
                byte[] raw=Fixture(name);ulong root=BitConverter.ToUInt64(raw,40);
                using(var b=new NativeArray<byte>(raw,Allocator.Temp)) Assert.That(ConsequencesDecoder.Decode(b,b.Length,geometry,root).RootTag,Is.EqualTo(BitConverter.ToUInt32(raw,96+32)));
                foreach(uint tag in new uint[]{0,5,6,12,15,17,18,19,21,22,23,99})
                {
                    byte[] copy=(byte[])raw.Clone();U32(copy,96+32,tag);
                    using(var b=new NativeArray<byte>(copy,Allocator.Temp)) Assert.Throws<ArgumentException>(()=>ConsequencesDecoder.Decode(b,b.Length,geometry,root),name+" root tag "+tag);
                }
            }
            // A known tag under another root's header rules is still refused.
            foreach(var (name,tag) in new[]{("consequences-v11",14u),("consequences-bless-v11",13u),("consequences-bless-v11",16u),("consequences-storm-v11",14u),("consequences-gathering-v11",16u)})
            {
                byte[] copy=Fixture(name);ulong root=BitConverter.ToUInt64(copy,40);U32(copy,96+32,tag);
                using(var b=new NativeArray<byte>(copy,Allocator.Temp)) Assert.Throws<ArgumentException>(()=>ConsequencesDecoder.Decode(b,b.Length,geometry,root),name+" as "+tag);
            }
            // Per-root header and row rules on real native bytes.
            byte[] bless=Fixture("consequences-bless-v11"),storm=Fixture("consequences-storm-v11"),gathering=Fixture("consequences-gathering-v11");
            foreach(var (raw,corrupt) in new (byte[],Action<byte[]>)[] {
                (bless,b=>U64(b,48,0)),(bless,b=>U64(b,48,BitConverter.ToUInt64(gathering,48))),(bless,b=>U32(b,76,1)),(bless,b=>U64(b,88,BitConverter.ToUInt64(b,88)+100000)),
                (bless,b=>U64(b,96+56,0)),(bless,b=>U32(b,96+128+32,15)),(bless,b=>U32(b,96+128+112,0)),(bless,b=>U32(b,96+128+80,1)),(bless,b=>U64(b,96+128+104,BitConverter.ToUInt64(b,40))),
                (storm,b=>U64(b,48,BitConverter.ToUInt64(bless,48))),(storm,b=>U32(b,76,5)),(storm,b=>U64(b,88,BitConverter.ToUInt64(b,88)+100000)),
                (storm,b=>U32(b,96+128+32,19)),(storm,b=>U32(b,96+256+32,18)),(storm,b=>U64(b,96+256+40,99)),(storm,b=>U32(b,96+128+96,3)),
                (gathering,b=>U64(b,48,0)),(gathering,b=>U64(b,48,BitConverter.ToUInt64(bless,48))),(gathering,b=>U32(b,76,1)),(gathering,b=>U64(b,96+64,8)),(gathering,b=>U64(b,96+56,60000000)) })
            {
                byte[] copy=(byte[])raw.Clone();corrupt(copy);ulong root=BitConverter.ToUInt64(copy,40);
                using(var b=new NativeArray<byte>(copy,Allocator.Temp)) Assert.Throws<ArgumentException>(()=>ConsequencesDecoder.Decode(b,b.Length,geometry,root));
            }
        }
        [Test]
        public void InspectionAcceptsKindThreeAwayFromHomeOnly()
        {
            // Schema3 (ADR0038 §4): kind3 may stand away from home only with the Home activity and
            // Socialize at a featured building; kind4 has its own storm-hold rule.
            var geometry=Geometry();byte[] raw=Fixture("inspect-resident-v11");ulong id=Id(raw);int state=96+(int)BitConverter.ToUInt32(raw,64);
            ulong home=BitConverter.ToUInt64(raw,104),elsewhere=0;foreach(ulong f in geometry.FeaturedIds) if(f!=home) {elsewhere=f;break;}
            byte[] Variant(uint kind,uint action,ulong at)
            {
                byte[] b=(byte[])raw.Clone();U32(b,state+80,kind);U64(b,state+88,kind==0?0UL:6UL);U32(b,state+8,action);U64(b,state+32,at);U64(b,state+40,at);return b;
            }
            SliceInspection Decode(byte[] bytes) {using(var b=new NativeArray<byte>(bytes,Allocator.Temp)) return InspectionDecoder.Decode(b,b.Length,geometry,id);}
            var gathered=Decode(Variant(3,3,elsewhere));Assert.That(gathered.OverrideKind,Is.EqualTo(3));Assert.That(gathered.From,Is.EqualTo(elsewhere));
            Assert.That(Decode(Variant(3,3,home)).OverrideKind,Is.EqualTo(3));Assert.That(Decode(Variant(3,1,home)).OverrideKind,Is.EqualTo(3));
            foreach(var bytes in new[]{Variant(1,1,elsewhere),Variant(2,3,elsewhere),Variant(3,1,elsewhere),Variant(3,4,home),Variant(5,3,home)})
                Assert.Throws<ArgumentException>(()=>Decode(bytes));
            // Kind4 (storm hold, D1 as amended) stands at a timetable place with the action ws-life
            // chooses for the kept activity (Home here); Travel or Sleep at home is incoherent.
            Assert.That(Decode(Variant(4,3,home)).OverrideKind,Is.EqualTo(4));
            foreach(var bytes in new[]{Variant(4,5,home),Variant(4,6,home)}) Assert.Throws<ArgumentException>(()=>Decode(bytes));
        }
        [Test]
        public void NativeInspectionMatchesSharedCoreFixturesAndEveryRenderedIdentityWithoutMutation()
        {
            using(var world=new NativeSlice(71449,Start))
            {
                world.SetCamera(Vector3.zero,500);world.Steps(20);var frame=new SliceFrame();world.Fill(frame,out _);
                ulong hash=frame.StateHash;
                foreach(string name in new[]{"inspect-resident-v11","inspect-building-v11"})
                {
                    byte[] raw=Fixture(name);SliceInspection expected;
                    using(var b=new NativeArray<byte>(raw,Allocator.Temp)) expected=InspectionDecoder.Decode(b,b.Length,world.Geometry,Id(raw));
                    var actual=world.Inspect(Id(raw));
                    Assert.That(actual.Tick,Is.EqualTo(expected.Tick));Assert.That(actual.Time,Is.EqualTo(expected.Time));Assert.That(actual.StateHash,Is.EqualTo(hash));
                    Assert.That(actual.Kind,Is.EqualTo(expected.Kind));Assert.That(actual.Sex,Is.EqualTo(expected.Sex));Assert.That(actual.Health,Is.EqualTo(expected.Health));Assert.That(actual.Life,Is.EqualTo(expected.Life));Assert.That(actual.GivenName,Is.EqualTo(expected.GivenName));Assert.That(actual.FamilyName,Is.EqualTo(expected.FamilyName));
                    Assert.That(actual.Address,Is.EqualTo(expected.Address));Assert.That(actual.Bin,Is.EqualTo(expected.Bin));Assert.That(actual.Deficits,Is.EqualTo(expected.Deficits));
                    Assert.That(actual.Schedule,Is.EqualTo(expected.Schedule));Assert.That(actual.Events,Is.EqualTo(expected.Events));Assert.That(actual.Summary,Is.EqualTo(expected.Summary));
                    Assert.That(actual.StateHash,Is.EqualTo(actual.Sky.StateHash));Assert.That(actual.Tick,Is.EqualTo(actual.Sky.Tick));
                }
                for(int i=0;i<frame.Population;i++)
                {
                    var r=world.Inspect(frame.Residents[i].Id);Assert.That(r.Kind,Is.EqualTo(1));Assert.That(r.Sex,Is.LessThanOrEqualTo(1));Assert.That(r.Health,Is.EqualTo(100));Assert.That(r.Life,Is.Zero);Assert.That(r.Detail,Is.EqualTo(frame.Residents[i].Detail));Assert.That(r.Activity,Is.EqualTo(frame.Residents[i].Activity));
                }
                for(int i=0;i<4;i++) Assert.That(world.Inspect(frame.Buildings[i].Id).Summary,Is.EqualTo(frame.Buildings[i]));
                Assert.Throws<InvalidOperationException>(()=>world.Inspect(0));world.Fill(frame,out _);Assert.That(frame.StateHash,Is.EqualTo(hash));Assert.That(frame.Tick,Is.EqualTo(20));
            }
        }
        [Test]
        public void MalformedInspectionNeverReplacesThePreviousCompleteRecord()
        {
            byte[] original=Fixture("inspect-resident-v11");var geometry=Geometry();SliceInspection published;
            using(var bytes=new NativeArray<byte>(original,Allocator.Temp)) published=InspectionDecoder.Decode(bytes,bytes.Length,geometry,Id(original));
            var saved=published;
            int state=96+(int)BitConverter.ToUInt32(original,64);
            foreach(Action<byte[]> damage in new Action<byte[]>[] {
                b=>U32(b,0,5),b=>U32(b,4,95),b=>U64(b,32,0),b=>U64(b,40,0),b=>U32(b,48,1),b=>U32(b,52,3),b=>U32(b,56,8193),b=>U32(b,60,2),
                b=>U32(b,64,uint.MaxValue),b=>U32(b,68,64),b=>U32(b,72,33),b=>U32(b,76,17),b=>U32(b,80,32),b=>U32(b,92,1),
                b=>U32(b,100,0),b=>U32(b,136,2),b=>U32(b,140,129),b=>b[144]=0xff,b=>U32(b,state,3),b=>U32(b,state+16,100001),b=>U32(b,state+56,101),b=>U32(b,state+56,0),b=>U32(b,state+60,1),b=>U32(b,state+60,2),b=>U32(b,state+76,2) })
            {
                byte[] raw=(byte[])original.Clone();damage(raw);
                using(var b=new NativeArray<byte>(raw,Allocator.Temp)) Assert.Catch<Exception>(()=>published=InspectionDecoder.Decode(b,b.Length,geometry,Id(original)));
                Assert.That(published,Is.SameAs(saved));
            }
            using(var b=new NativeArray<byte>(original,Allocator.Temp)) Assert.Catch<Exception>(()=>InspectionDecoder.Decode(b,b.Length-1,geometry,Id(original)));
            int scheduleAt=state+112,commute=-1;
            for(int i=0;i<BitConverter.ToUInt32(original,72);i++) if(BitConverter.ToUInt32(original,scheduleAt+i*48+16)!=0) {commute=scheduleAt+i*48;break;}
            Assert.That(commute,Is.GreaterThanOrEqualTo(0));
            foreach(Action<byte[]> damage in new Action<byte[]>[] {
                b=>U32(b,commute+20,BitConverter.ToUInt32(original,commute+20)+1),
                b=>{U64(b,commute+32,BitConverter.ToUInt64(original,commute+40));U32(b,commute+12,BitConverter.ToUInt32(original,commute+28)==uint.MaxValue?0u:1u);U32(b,commute+24,BitConverter.ToUInt32(original,commute+28));},
                b=>U32(b,state+4,(BitConverter.ToUInt32(original,state+4)+1)%6) })
            {
                byte[] raw=(byte[])original.Clone();damage(raw);
                using(var b=new NativeArray<byte>(raw,Allocator.Temp)) Assert.Catch<Exception>(()=>InspectionDecoder.Decode(b,b.Length,geometry,Id(original)));
            }
        }
        [Test]
        public void ResidentPanelShowsOnlyRequestedFieldsFromCoreIncludingBothSexesAndLifeStates()
        {
            var resident=new SliceInspection {Kind=1,Age=35,GivenName="Eden",FamilyName="Singh",Home=123,Workplace=456,Role=2,Sex=0,Health=100,Life=0};
            resident.Deficits[0]=99999;
            Assert.That(SliceInspectionText.Format(resident),Is.EqualTo(
                "Age: 35\nSex: Female\nEmployment: Employed\nHealth: 100/100\nAlive/dead: Alive"));
            foreach(var row in new[]{(0u,"Not employed"),(1u,"Student"),(2u,"Employed"),(3u,"Retired")})
            {
                resident.Role=row.Item1;
                Assert.That(SliceInspectionText.Format(resident).Split('\n'),Is.EqualTo(new[]{
                    "Age: 35","Sex: Female","Employment: "+row.Item2,"Health: 100/100","Alive/dead: Alive"}));
            }
        }
        [Test]
        public void ConstructedVitalRecordsDisplayMaleUnwellAndDeadValues()
        {
            var r=new SliceInspection {Kind=1,Age=65,Sex=1,Role=3,Health=75,Life=0};
            Assert.That(SliceInspectionText.Format(r),Is.EqualTo("Age: 65\nSex: Male\nEmployment: Retired\nHealth: 75/100\nAlive/dead: Alive"));
            r.Health=0;r.Life=1;
            Assert.That(SliceInspectionText.Format(r),Is.EqualTo("Age: 65\nSex: Male\nEmployment: Retired\nHealth: 0/100\nAlive/dead: Dead"));
        }
        [Test]
        public void TapThresholdsCancelReturningDragsSecondTouchesUiAndLongPresses()
        {
            var tap=new SliceTapGesture();
            tap.Begin(1,Vector2.zero,0,false);Assert.That(tap.End(1,new Vector2(8,0),0.35,false),Is.True);
            tap.Begin(1,Vector2.zero,0,false);tap.Move(1,new Vector2(9,0),false);Assert.That(tap.End(1,Vector2.zero,0.2,false),Is.False);
            tap.Begin(1,Vector2.zero,0,false);tap.Cancel();Assert.That(tap.End(1,Vector2.zero,0.2,false),Is.False);
            tap.Begin(1,Vector2.zero,0,true);Assert.That(tap.UiCaptured,Is.True);Assert.That(tap.End(1,Vector2.zero,0.1,false),Is.False);
            tap.Begin(1,Vector2.zero,0,false);tap.Move(1,Vector2.one,true);Assert.That(tap.End(1,Vector2.zero,0.1,false),Is.False);
            tap.Begin(1,Vector2.zero,0,false);Assert.That(tap.End(1,Vector2.zero,0.351,false),Is.False);
            tap.Begin(1,Vector2.zero,1,false);Assert.That(tap.End(1,Vector2.zero,0,false),Is.False);
            tap.Begin(1,Vector2.zero,0,false);Assert.That(tap.End(1,new Vector2(float.NaN,0),0.1,false),Is.False);
        }
        [UnityTest]
        public IEnumerator RealSceneTapsInspectBuildingAndResidentAndNeverPollTheSelection()
        {
            yield return new EnterPlayMode();
            var loading=EditorSceneManager.LoadSceneAsyncInPlayMode("Assets/WorldSim/Scenes/RealBlock.unity",new LoadSceneParameters(LoadSceneMode.Single));
            while(!loading.isDone) yield return null;
            var demo=Object.FindFirstObjectByType<SliceDemo>();Assert.That(demo,Is.Not.Null);demo.SetSpeed(0);yield return null;
            var mesh=ToyMeshes.Capsule();var picker=new SlicePicker(demo.Geometry,mesh);bool selected=false;
            foreach(var row in demo.Geometry.Buildings)
            {
                if(!row.Featured) continue;
                for(int i=row.FirstIndex;i<row.FirstIndex+row.IndexCount;i+=3)
                {
                    Vector3 centre=(demo.Geometry.Vertices[demo.Geometry.Indices[i]]+demo.Geometry.Vertices[demo.Geometry.Indices[i+1]]+demo.Geometry.Vertices[demo.Geometry.Indices[i+2]])/3;
                    Vector3 point=demo.SceneCamera.WorldToScreenPoint(centre);if(point.z<=0) continue;
                    if(!picker.TryPick(demo.SceneCamera.ScreenPointToRay(point),demo.CurrentFrame,out var hit)||hit.Resident||hit.Id==0) continue;
                    if(!demo.InspectScreenPoint(point)) continue;
                    Assert.That(demo.CurrentInspection.Id,Is.EqualTo(hit.Id));Assert.That(demo.CurrentInspection.Kind,Is.EqualTo(2));selected=true;break;
                }
                if(selected) break;
            }
            Assert.That(selected,Is.True,"A visible featured building must be inspectable in the actual scene.");
            int calls=demo.InspectionQueries;ulong sampledTick=demo.CurrentInspection.Tick;string text=demo.InspectorText;
            for(int i=0;i<6;i++) yield return null;
            Assert.That(demo.InspectionQueries,Is.EqualTo(calls));Assert.That(demo.CurrentInspection.Tick,Is.EqualTo(sampledTick));Assert.That(demo.InspectorText,Is.EqualTo(text));
            File.WriteAllText(Path.Combine(Path.GetTempPath(),"worldsim73-building-inspection.txt"),text);
            UnityEditorInternal.InternalEditorUtility.RepaintAllViews();
            yield return new WaitForSecondsRealtime(0.5f);
            yield return CaptureInspection("building");
            demo.ClearInspection();demo.SetSpeed(1);demo.RequestStreetView();double deadline=Time.realtimeSinceStartupAsDouble+8;
            while(demo.WaitingForStreet&&Time.realtimeSinceStartupAsDouble<deadline) yield return null;
            Assert.That(demo.WaitingForStreet,Is.False);demo.SetSpeed(0);yield return null;selected=false;
            for(int i=0;i<demo.CurrentFrame.RenderedResidents;i++)
            {
                Vector3 centre=demo.CurrentFrame.ResidentMatrices[i].MultiplyPoint3x4(Vector3.zero),point=demo.SceneCamera.WorldToScreenPoint(centre);
                if(point.z<=0||!picker.TryPick(demo.SceneCamera.ScreenPointToRay(point),demo.CurrentFrame,out var hit)||!hit.Resident) continue;
                if(!demo.InspectScreenPoint(point)) continue;
                Assert.That(demo.CurrentInspection.Id,Is.EqualTo(hit.Id));Assert.That(demo.CurrentInspection.Kind,Is.EqualTo(1));selected=true;break;
            }
            Assert.That(selected,Is.True,"An actually visible local fine resident must be inspectable.");
            Assert.That(demo.InspectorText,Is.EqualTo(SliceInspectionText.Format(demo.CurrentInspection)));
            Assert.That(demo.InspectorText.Split('\n'),Has.Length.EqualTo(5));Assert.That(demo.InspectorText,Does.Contain("Health: 100/100"));
            File.WriteAllText(Path.Combine(Path.GetTempPath(),"worldsim73-resident-inspection.txt"),demo.InspectorText);
            UnityEditorInternal.InternalEditorUtility.RepaintAllViews();
            yield return new WaitForSecondsRealtime(0.5f);
            yield return CaptureInspection("resident");
            Assert.That(demo.InspectIllness(),Is.True);Assert.That(demo.CurrentInspection.Health,Is.EqualTo(55));
            string illnessText=demo.InspectorText;StringAssert.Contains("Illness began",illnessText);StringAssert.Contains("Direct",illnessText);
            File.WriteAllText(Path.Combine(Path.GetTempPath(),"worldsim74-illness.txt"),illnessText);
            yield return new WaitForSecondsRealtime(0.5f);yield return CaptureInspection("illness");
            sampledTick=demo.CurrentInspection.Tick;for(int i=0;i<6;i++) yield return null;
            Assert.That(demo.CurrentInspection.Tick,Is.EqualTo(sampledTick));Assert.That(demo.InspectorText,Is.EqualTo(illnessText));
            Assert.That(demo.InspectIllness(),Is.True);Assert.That(demo.InspectorText,Is.EqualTo(illnessText));
            demo.SetSpeed(100);deadline=Time.realtimeSinceStartupAsDouble+110;
            // The production button applies two model hours: at100x this takes72 seconds.
            long end=demo.CurrentInspection.IllnessEnd;
            while(demo.CurrentFrame.Sky.SimTimeMicroseconds<end&&Time.realtimeSinceStartupAsDouble<deadline) yield return null;
            Assert.That(demo.CurrentFrame.Sky.SimTimeMicroseconds,Is.GreaterThanOrEqualTo(end));demo.SetSpeed(0);
            Assert.That(demo.InspectIllness(),Is.True);Assert.That(demo.CurrentInspection.Health,Is.EqualTo(100));StringAssert.Contains("Recovered",demo.InspectorText);
            File.WriteAllText(Path.Combine(Path.GetTempPath(),"worldsim74-recovery.txt"),demo.InspectorText);
            yield return new WaitForSecondsRealtime(0.5f);yield return CaptureInspection("recovery");
            calls=demo.InspectionQueries;Assert.That(demo.InspectScreenPoint(new Vector2(10,Screen.height-10)),Is.False);Assert.That(demo.InspectionQueries,Is.EqualTo(calls));
            demo.enabled=false;Assert.That(demo.CurrentInspection,Is.Null);Assert.That(demo.InspectorText,Is.Empty);Object.DestroyImmediate(mesh);
            yield return new ExitPlayMode();
        }
        static IEnumerator CaptureInspection(string kind)
        {
            // Batch mode does not composite the Game view. The visible run must
            // finish writing this frame before selection/camera state changes.
            if(Application.isBatchMode) yield break;
            string path=Path.Combine(Path.GetTempPath(),(kind=="illness"||kind=="recovery"?"worldsim74-":"worldsim73-")+kind+"-inspection.png");
            if(File.Exists(path)) File.Delete(path);
            ScreenCapture.CaptureScreenshot(path);
            double deadline=Time.realtimeSinceStartupAsDouble+5;
            while(!File.Exists(path)&&Time.realtimeSinceStartupAsDouble<deadline) yield return null;
            Assert.That(File.Exists(path),Is.True,"The selected inspector frame must be captured before changing selection.");
            yield return new WaitForSecondsRealtime(0.25f);
        }
        [Test]
        public void ContextOccludesHiddenCapsulesAndEqualDepthUsesStableResidentIdentity()
        {
            var geometry=Geometry();var mesh=ToyMeshes.Capsule();
            try
            {
                var picker=new SlicePicker(geometry,mesh);var frame=new SliceFrame();Vector3 roof=default;bool found=false;
                foreach(var row in geometry.Buildings)
                {
                    if(row.Featured) continue;
                    for(int i=row.FirstIndex;i<row.FirstIndex+row.IndexCount;i+=3)
                    {
                        int a=geometry.Indices[i],b=geometry.Indices[i+1],c=geometry.Indices[i+2];
                        if(geometry.Normals[a].y<0.9f) continue;roof=(geometry.Vertices[a]+geometry.Vertices[b]+geometry.Vertices[c])/3;found=true;break;
                    }
                    if(found) break;
                }
                Assert.That(found,Is.True);var ray=new Ray(roof+Vector3.up*200,Vector3.down);
                frame.RenderedResidents=1;frame.RenderedResidentIds[0]=100;frame.ResidentMatrices[0]=Matrix4x4.TRS(roof-Vector3.up*3,Quaternion.identity,Vector3.one);
                Assert.That(picker.TryPick(ray,frame,out var hit),Is.True);Assert.That(hit.Id,Is.Zero,"A context footprint must occlude the hidden resident.");
                frame.ResidentMatrices[0]=Matrix4x4.TRS(roof+Vector3.up*100,Quaternion.identity,Vector3.one);
                Assert.That(picker.TryPick(ray,frame,out hit),Is.True);Assert.That(hit.Resident,Is.True);Assert.That(hit.Id,Is.EqualTo(100));
                frame.RenderedResidents=2;frame.RenderedResidentIds[1]=99;frame.ResidentMatrices[1]=frame.ResidentMatrices[0];
                Assert.That(picker.TryPick(ray,frame,out hit),Is.True);Assert.That(hit.Id,Is.EqualTo(99));
                // Nonuniform capsule scale must not normalize away world-ray t.
                frame.RenderedResidents=1;frame.RenderedResidentIds[0]=100;
                var nearRoof=new Ray(roof+Vector3.up*10,Vector3.down);
                frame.ResidentMatrices[0]=Matrix4x4.TRS(roof+Vector3.up*0.5f,Quaternion.identity,new Vector3(0.5f,0.85f,0.5f));
                Assert.That(picker.TryPick(nearRoof,frame,out hit),Is.True);Assert.That(hit.Resident,Is.True,"The scaled capsule cap is in front of the roof.");
                frame.ResidentMatrices[0]=Matrix4x4.TRS(roof-Vector3.up*3,Quaternion.identity,Vector3.one*2);
                Assert.That(picker.TryPick(nearRoof,frame,out hit),Is.True);Assert.That(hit.Resident,Is.False,"The enlarged capsule is fully hidden behind the roof.");
                frame.RenderedResidents=0;Assert.That(picker.TryPick(ray,frame,out hit),Is.True);Assert.That(hit.Resident,Is.False);
                Assert.That(picker.TryPick(new Ray(Vector3.zero,Vector3.zero),frame,out _),Is.False);
            }
            finally {Object.DestroyImmediate(mesh);}
        }
    }
}
