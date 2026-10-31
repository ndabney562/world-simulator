using System;
using System.IO;
using System.Runtime.InteropServices;
using NUnit.Framework;
using Unity.Collections;
using UnityEngine;
using WorldSim.Bridge;
using WorldSim.Rendering;

namespace WorldSim.Tests.EditMode
{
    public sealed class SliceBridgeTests
    {
        static long Start => NativeWorld.UtcToSim(2026,6,22,11,22,30);
        static byte[] Fixture(string name)
        {
            string hex=File.ReadAllText(Path.Combine(Application.dataPath,"../../../core/crates/ws-ffi/tests/fixtures/"+name+".hex")).Replace("\n","").Replace("\r","").Replace(" ","");
            var result=new byte[hex.Length/2];for(int i=0;i<result.Length;i++) result[i]=Convert.ToByte(hex.Substring(i*2,2),16);return result;
        }
        static void U32(byte[] bytes,int at,uint value) {Array.Copy(BitConverter.GetBytes(value),0,bytes,at,4);}
        static void U64(byte[] bytes,int at,ulong value) {Array.Copy(BitConverter.GetBytes(value),0,bytes,at,8);}
        static SliceGeometry Geometry()
        {
            using(var bytes=new NativeArray<byte>(Fixture("slice-geometry-v9"),Allocator.Temp)) return SliceDecoder.DecodeGeometry(bytes,bytes.Length);
        }
        [Test]
        public void SliceConfigAndOperationMatchEveryNativeOffsetAndRefuseOldClients()
        {
            Assert.That(Marshal.SizeOf<NativeSliceConfig>(),Is.EqualTo(32));
            string[] config={"Version","Size","Seed","Start","Reserved"};int[] offsets={0,4,8,16,24};
            for(int i=0;i<config.Length;i++) Assert.That(Marshal.OffsetOf<NativeSliceConfig>(config[i]).ToInt32(),Is.EqualTo(offsets[i]));
            Assert.That(Marshal.SizeOf<NativeSliceOperation>(),Is.EqualTo(64));
            string[] operation={"Version","Size","Tag","Reserved","A","B","C","D","E","F"};int[] operationOffsets={0,4,8,12,16,24,32,40,48,56};
            for(int i=0;i<operation.Length;i++) Assert.That(Marshal.OffsetOf<NativeSliceOperation>(operation[i]).ToInt32(),Is.EqualTo(operationOffsets[i]));
            for(uint old=1;old<=8;old++)
            {
                Assert.Throws<InvalidOperationException>(()=>NativeWorld.RequireVersion(old));
                var input=new NativeSliceConfig {Version=old,Size=32,Seed=71449,Start=Start};var pointer=NativeSliceMethods.Create(in input);
                if(pointer!=IntPtr.Zero) NativeSliceMethods.Destroy(pointer);
                Assert.That(pointer,Is.EqualTo(IntPtr.Zero));
            }
        }
        [Test]
        public void SharedRealSliceFixturesMatchNativeGeometryIdentitiesPositionsCountsAndSky()
        {
            var expectedGeometry=Geometry();var expected=new SliceFrame();
            using(var fixture=new NativeArray<byte>(Fixture("slice-snapshot-v9"),Allocator.Temp)) SliceDecoder.DecodeSnapshot(fixture,fixture.Length,expectedGeometry,expected);
            Assert.That(expectedGeometry.Buildings.Length,Is.EqualTo(58));Assert.That(expectedGeometry.FeaturedIds.Length,Is.EqualTo(4));
            Assert.That(expectedGeometry.Vertices.Length,Is.EqualTo(2245));Assert.That(expectedGeometry.Indices.Length,Is.EqualTo(3693));
            Assert.That(expected.Population,Is.EqualTo(30));Assert.That(expected.Tick,Is.EqualTo(20));Assert.That(expected.StateHash,Is.Zero);
            using(var world=new NativeSlice(71449,Start))
            {
                Assert.That(world.Population,Is.EqualTo(30));
                for(int i=0;i<expectedGeometry.Buildings.Length;i++)
                {
                    Assert.That(world.Geometry.Buildings[i].Id,Is.EqualTo(expectedGeometry.Buildings[i].Id));
                    Assert.That(world.Geometry.Buildings[i].Bin,Is.EqualTo(expectedGeometry.Buildings[i].Bin));
                }
                for(int i=0;i<expectedGeometry.Vertices.Length;i++) Assert.That(Vector3.Distance(world.Geometry.Vertices[i],expectedGeometry.Vertices[i]),Is.LessThan(0.0002f));
                CollectionAssert.AreEqual(world.Geometry.Indices,expectedGeometry.Indices);
                world.SetCamera(Vector3.zero,500);world.Steps(20);var frame=new SliceFrame();world.Fill(frame,out _);
                Assert.That(frame.Tick,Is.EqualTo(expected.Tick));Assert.That(frame.SimTimeMicroseconds,Is.EqualTo(expected.SimTimeMicroseconds));Assert.That(frame.L1,Is.EqualTo(expected.L1));
                for(int i=0;i<30;i++)
                {
                    Assert.That(frame.Residents[i].Id,Is.EqualTo(expected.Residents[i].Id));Assert.That(frame.Residents[i].Governing,Is.EqualTo(expected.Residents[i].Governing));
                    Assert.That(frame.Residents[i].Location,Is.EqualTo(expected.Residents[i].Location));Assert.That(Vector3.Distance(frame.Residents[i].Foot,expected.Residents[i].Foot),Is.LessThan(0.0002f));
                }
                Assert.That(frame.RenderedResidents,Is.GreaterThan(0));Assert.That(frame.StateHash,Is.EqualTo(frame.Sky.StateHash));Assert.That(frame.Tick,Is.EqualTo(frame.Sky.Tick));
                byte[] skyBytes=Fixture("slice-sky-v9");var pinned=GCHandle.Alloc(skyBytes,GCHandleType.Pinned);
                try
                {
                    var sky=Marshal.PtrToStructure<CalendarSky>(pinned.AddrOfPinnedObject());sky.Validate();Assert.That(sky.StateHash,Is.Zero);
                    Assert.That(frame.Sky.SunEast,Is.EqualTo(sky.SunEast));Assert.That(frame.Sky.SunNorth,Is.EqualTo(sky.SunNorth));Assert.That(frame.Sky.SunUp,Is.EqualTo(sky.SunUp));
                }
                finally {pinned.Free();}
            }
        }
        [Test]
        public void InvalidGeometryIsRejectedBeforeAnyPartialCacheIsPublished()
        {
            byte[] original=Fixture("slice-geometry-v9");
            foreach(Action<byte[]> damage in new Action<byte[]>[] {
                b=>U32(b,0,4),b=>U32(b,20,uint.MaxValue),b=>U32(b,24,uint.MaxValue),b=>U32(b,36,25),b=>U32(b,44,1),
                b=>U64(b,96+48,BitConverter.ToUInt64(b,96)),b=>U32(b,96+36,2),b=>U32(b,96+16,1),
                b=>U32(b,96+58*48,0x7fc00000),b=>U32(b,96+58*48+2245*24,uint.MaxValue) })
            {
                byte[] data=(byte[])original.Clone();damage(data);
                using(var bytes=new NativeArray<byte>(data,Allocator.Temp)) Assert.Catch<Exception>(()=>SliceDecoder.DecodeGeometry(bytes,bytes.Length));
            }
            using(var bytes=new NativeArray<byte>(original,Allocator.Temp)) Assert.Throws<ArgumentException>(()=>SliceDecoder.DecodeGeometry(bytes,bytes.Length-1));
        }
        [Test]
        public void InvalidDynamicRowsLeaveTheCompletePreviousFrameUntouched()
        {
            var geometry=Geometry();byte[] original=Fixture("slice-snapshot-v9");var frame=new SliceFrame();
            using(var bytes=new NativeArray<byte>(original,Allocator.Temp)) SliceDecoder.DecodeSnapshot(bytes,bytes.Length,geometry,frame);
            ulong tick=frame.Tick,first=frame.Residents[0].Id;Matrix4x4 matrix=frame.ResidentMatrices[0];
            foreach(Action<byte[]> damage in new Action<byte[]>[] {
                b=>U32(b,0,4),b=>U32(b,48,uint.MaxValue),b=>U32(b,60,49),b=>U32(b,92,1),b=>U64(b,128+48,first),
                b=>U64(b,128+8,0),b=>U64(b,128,geometry.FeaturedIds[0]),b=>U32(b,128+32,3),b=>U32(b,128+36,6),b=>U32(b,128+40,4),b=>U32(b,128+28,0x7fc00000),
                b=>U32(b,128+30*48+16,31),b=>U32(b,128+30*48+44,1) })
            {
                byte[] data=(byte[])original.Clone();damage(data);
                using(var bytes=new NativeArray<byte>(data,Allocator.Temp)) Assert.Catch<Exception>(()=>SliceDecoder.DecodeSnapshot(bytes,bytes.Length,geometry,frame));
                Assert.That(frame.Tick,Is.EqualTo(tick));Assert.That(frame.Residents[0].Id,Is.EqualTo(first));Assert.That(frame.ResidentMatrices[0],Is.EqualTo(matrix));
            }
        }
        [Test]
        public void PurePairedQueriesPauseAndOverviewRetainAllPeopleAndNeverInventFinePositions()
        {
            using(var world=new NativeSlice(71449,Start))
            {
                var frame=new SliceFrame();world.Fill(frame,out _);ulong before=frame.StateHash;var ids=new ulong[30];for(int i=0;i<30;i++) ids[i]=frame.Residents[i].Id;
                for(int i=0;i<5;i++) {world.Fill(frame,out _);Assert.That(frame.StateHash,Is.EqualTo(before));Assert.That(frame.Tick,Is.Zero);}
                world.SetCamera(Vector3.zero,500);world.Steps(20);world.Fill(frame,out _);Assert.That(frame.RenderedResidents,Is.GreaterThan(0));
                world.SetScale(0);world.HostElapsed(1000000);world.Fill(frame,out _);Assert.That(frame.Tick,Is.EqualTo(20));Assert.That(frame.Scale,Is.Zero);
                world.SetScale(100);world.HostElapsed(16667);world.Fill(frame,out var stats);Assert.That(frame.Tick,Is.EqualTo(36));Assert.That(frame.L1,Is.Zero);Assert.That(frame.RenderedResidents,Is.Zero);Assert.That(stats.CommittedTicks,Is.EqualTo(16));
                world.SetScale(1);world.SetCamera(Vector3.zero,0);world.Steps(20);world.Fill(frame,out _);
                Assert.That(frame.Population,Is.EqualTo(30));Assert.That(frame.L2,Is.EqualTo(30));
                for(int i=0;i<30;i++) {Assert.That(frame.Residents[i].Id,Is.EqualTo(ids[i]));Assert.That(frame.Residents[i].Location,Is.Zero);Assert.That(frame.Residents[i].Foot,Is.EqualTo(Vector3.zero));}
                world.Dispose();Assert.Throws<ObjectDisposedException>(()=>world.Fill(frame,out _));
            }
        }
        [Test]
        public void WarmedRealSliceFillDecodeAndCoherentSkyAllocateNoManagedBytes()
        {
            using(var world=new NativeSlice(71449,Start))
            {
                var frame=new SliceFrame();for(int i=0;i<10;i++) world.Fill(frame,out _);
                long before=GC.GetAllocatedBytesForCurrentThread();for(int i=0;i<20;i++) world.Fill(frame,out _);
                Assert.That(GC.GetAllocatedBytesForCurrentThread()-before,Is.Zero);
            }
        }
        [Test]
        public void TwentyMinuteHostCadenceIsBoundedAndPausedTimeNeverReachesTheCore()
        {
            var clock=new SliceHostClock(1000000,0);long now=0;int calls=0;ulong micros=0;
            using(var world=new NativeSlice(71449,Start))
            {
                var frame=new SliceFrame();
                // Twenty minutes of2ms scheduler polls, without wall-clock waiting.
                for(int i=0;i<600000;i++)
                {
                    now+=2000;
                    if(clock.Advance(now,false,out ulong elapsed)) {calls++;micros+=elapsed;world.HostElapsed(elapsed);}
                }
                world.Fill(frame,out _);
                Assert.That(calls,Is.EqualTo(24000),"Retained host records must be bounded to20Hz, not500Hz.");
                Assert.That(micros,Is.EqualTo(1200000000UL));Assert.That(frame.Tick,Is.EqualTo(12000UL));
                world.SetScale(0);clock.Paused=true;ulong pausedHash=frame.StateHash;
                world.Fill(frame,out _);pausedHash=frame.StateHash;
                for(int i=0;i<600000;i++)
                {
                    now+=2000;
                    Assert.That(clock.Advance(now,false,out _),Is.False,"Paused time must produce no native operation records.");
                }
                world.Fill(frame,out _);Assert.That(frame.StateHash,Is.EqualTo(pausedHash));Assert.That(frame.Tick,Is.EqualTo(12000UL));
                // Close the paused interval before resuming, exactly as SetScale does.
                now+=17000;Assert.That(clock.Advance(now,true,out _),Is.False);
                world.SetScale(100);clock.Paused=false;
                for(int i=0;i<500;i++) {now+=2000;if(clock.Advance(now,false,out ulong elapsed)) world.HostElapsed(elapsed);}
                world.Fill(frame,out _);Assert.That(frame.Tick,Is.EqualTo(13000UL),"Only one resumed second is attributed to100x.");
            }
        }
        [Test]
        public void HostClockClosesOldScaleIntervalsAndPreservesFractionalStopwatchUnits()
        {
            var clock=new SliceHostClock(3000001,0);ulong total=0;
            // Sub-cadence control changes flush at their actual timestamp, not the next worker wake.
            Assert.That(clock.Advance(10000,false,out _),Is.False);
            Assert.That(clock.Advance(10000,true,out ulong beforePause),Is.True);total+=beforePause;
            clock.Paused=true;Assert.That(clock.Advance(999999999,true,out _),Is.False);clock.Paused=false;
            Assert.That(clock.Advance(1000009999,true,out ulong afterResume),Is.True);total+=afterResume;
            Assert.That(total,Is.EqualTo(20000UL*1000000/3000001));
            Assert.That(clock.Advance(1000009999,true,out _),Is.False);
            Assert.Throws<InvalidOperationException>(()=>clock.Advance(1000009998,false,out _));
            Assert.Throws<ArgumentOutOfRangeException>(()=>new SliceHostClock(0,0));
        }
        [Test]
        public void SliceSaveAndLoadRoundTripReproducesStateHashAndContinuesIdentically()
        {
            string path=Path.Combine(Path.GetTempPath(),"unity_slice_rt_"+Guid.NewGuid().ToString("N")+".wssave");
            try
            {
                var frameA=new SliceFrame();
                var frameB=new SliceFrame();
                using(var worldA=new NativeSlice(71449,Start))
                {
                    worldA.SetCamera(Vector3.zero,500);
                    worldA.Steps(50);
                    worldA.Fill(frameA,out _);
                    ulong targetId=frameA.Residents[0].Id;
                    var sampleA=worldA.Illness(targetId);
                    worldA.Steps(20);
                    worldA.Fill(frameA,out _);

                    worldA.Save(path);
                    Assert.That(File.Exists(path),Is.True);

                    using(var worldB=NativeSlice.Load(path))
                    {
                        Assert.That(worldB.Population,Is.EqualTo(worldA.Population));
                        worldB.Fill(frameB,out _);
                        Assert.That(frameB.Tick,Is.EqualTo(frameA.Tick));
                        Assert.That(frameB.SimTimeMicroseconds,Is.EqualTo(frameA.SimTimeMicroseconds));
                        Assert.That(frameB.StateHash,Is.EqualTo(frameA.StateHash));
                        Assert.That(frameB.Sky.StateHash,Is.EqualTo(frameA.Sky.StateHash));
                        for(int i=0;i<worldA.Population;i++)
                        {
                            Assert.That(frameB.Residents[i].Id,Is.EqualTo(frameA.Residents[i].Id));
                            Assert.That(frameB.Residents[i].Location,Is.EqualTo(frameA.Residents[i].Location));
                            Assert.That(Vector3.Distance(frameB.Residents[i].Foot,frameA.Residents[i].Foot),Is.LessThan(0.0002f));
                        }

                        var sampleB=worldB.Illness(targetId,sampleA.Consequences.Root);
                        Assert.That(sampleB.Consequences.Root,Is.EqualTo(sampleA.Consequences.Root));
                        Assert.That(sampleB.Consequences.Events,Is.EqualTo(sampleA.Consequences.Events));

                        worldA.Steps(30);
                        worldB.Steps(30);
                        worldA.Fill(frameA,out _);
                        worldB.Fill(frameB,out _);
                        Assert.That(frameB.StateHash,Is.EqualTo(frameA.StateHash));
                        Assert.That(frameB.Tick,Is.EqualTo(frameA.Tick));
                    }
                }
            }
            finally
            {
                if(File.Exists(path)) File.Delete(path);
            }
        }
        [Test]
        public void SlicePacedSaveRetainsHistoricalRemaindersAndContinuesAfterLoad()
        {
            string path=Path.Combine(Path.GetTempPath(),"unity_paced_"+Guid.NewGuid().ToString("N")+".wssave");
            try
            {
                using(var original=new NativeSlice(71449,Start))
                {
                    original.HostElapsed(90001);original.HostElapsed(20123);original.SetScale(10);
                    original.HostElapsed(58317);original.SetScale(0);original.Save(path);
                    using(var loaded=NativeSlice.Load(path))
                    {
                        var a=new SliceFrame();var b=new SliceFrame();
                        original.Fill(a,out _);loaded.Fill(b,out _);
                        Assert.That(b.StateHash,Is.EqualTo(a.StateHash));Assert.That(b.Tick,Is.EqualTo(a.Tick));
                        original.SetScale(100);loaded.SetScale(100);
                        original.HostElapsed(61337);loaded.HostElapsed(61337);
                        original.Fill(a,out _);loaded.Fill(b,out _);
                        Assert.That(b.StateHash,Is.EqualTo(a.StateHash));Assert.That(b.Tick,Is.EqualTo(a.Tick));
                    }
                }
            }
            finally {if(File.Exists(path)) File.Delete(path);}
        }
        [Test]
        public void SliceBackgroundSaveRunsAndReportsStatus()
        {
            string path=Path.Combine(Path.GetTempPath(),"unity_slice_bg_"+Guid.NewGuid().ToString("N")+".wssave");
            try
            {
                using(var world=new NativeSlice(71449,Start))
                {
                    world.Steps(25);
                    world.SaveBackground(path);
                    uint status=world.SaveStatus();
                    int polls=0;
                    while(status==NativeSlice.SaveInProgress&&polls++<100)
                    {
                        System.Threading.Thread.Sleep(5);
                        status=world.SaveStatus();
                    }
                    Assert.That(status,Is.Zero);
                    Assert.That(File.Exists(path),Is.True);

                    using(var loaded=NativeSlice.Load(path))
                    {
                        var frame=new SliceFrame();
                        loaded.Fill(frame,out _);
                        Assert.That(frame.Tick,Is.EqualTo(25));
                        Assert.That(loaded.Population,Is.EqualTo(30));
                    }
                }
            }
            finally
            {
                if(File.Exists(path)) File.Delete(path);
            }
        }
        static SliceConsequences DecodeFixture(string name,SliceGeometry geometry)
        {
            byte[] raw=Fixture(name);
            using(var b=new NativeArray<byte>(raw,Allocator.Temp)) return ConsequencesDecoder.Decode(b,b.Length,geometry,BitConverter.ToUInt64(raw,40));
        }
        static uint[] Tags(SliceConsequences c) {var tags=new uint[c.Events.Length];for(int i=0;i<tags.Length;i++) tags[i]=c.Events[i].Tag;return tags;}
        static int Count(SliceConsequences c,uint tag) {int n=0;foreach(var e in c.Events) if(e.Tag==tag) n++;return n;}
        [Test]
        public void BlessThenConsequencesDecode()
        {
            // ADR0038 §2: native bytes for a bless curing the shared sample illness.
            var fixture=DecodeFixture("consequences-bless-v9",Geometry());
            Assert.That(fixture.RootTag,Is.EqualTo(14));Assert.That(Tags(fixture),Is.EqualTo(new uint[]{14,12}));
            Assert.That(fixture.Severity,Is.Zero);Assert.That(fixture.End,Is.EqualTo(fixture.Applied));
            Assert.That(fixture.Causes[1].Length,Is.EqualTo(2));Assert.That(fixture.Causes[1][0].Parent,Is.EqualTo(fixture.Root));
            Assert.That(fixture.Causes[1][1].Relation,Is.EqualTo(3));Assert.That(fixture.Causes[1][1].Parent,Is.LessThan(fixture.Root));
            using(var world=new NativeSlice(71449,Start))
            {
                var frame=new SliceFrame();world.Fill(frame,out _);ulong id=frame.Residents[0].Id;
                ulong relief=world.Bless(id);var relieved=world.Consequences(relief);
                Assert.That(relieved.Root,Is.EqualTo(relief));Assert.That(relieved.RootTag,Is.EqualTo(14));Assert.That(relieved.Subject,Is.EqualTo(id));
                Assert.That(Tags(relieved),Is.EqualTo(new uint[]{14,15}));Assert.That(relieved.Events[0].C,Is.EqualTo(1));
                // Deficits are now zero: an accepted no-op bless has only its root.
                var noop=world.Consequences(world.Bless(id));Assert.That(Tags(noop),Is.EqualTo(new uint[]{14}));Assert.That(noop.Events[0].C,Is.Zero);
                var ill=world.Illness(id,0,5,600000000);ulong cure=world.Bless(id);
                Assert.That(cure,Is.GreaterThan(ill.Consequences.Root));
                var cured=world.Consequences(cure);Assert.That(Tags(cured),Is.EqualTo(new uint[]{14,12}));
                Assert.That(cured.Causes[1][1].Parent,Is.EqualTo(ill.Consequences.Events[1].Id),"The cure contributes from the original illness event.");
                Assert.That(world.Inspect(id).Health,Is.EqualTo(100));Assert.That(world.Inspect(id).IllnessActive,Is.False);
                // A semantic rejection throws through the shared apply path and records no root.
                ulong building=frame.Buildings[0].Id;var before=new SliceFrame();world.Fill(before,out _);
                Assert.Throws<InvalidOperationException>(()=>world.Bless(building));
                var after=new SliceFrame();world.Fill(after,out _);Assert.That(after.StateHash,Is.EqualTo(before.StateHash));
                Assert.That(world.Consequences(cure).Events.Length,Is.EqualTo(2));
                Assert.Throws<InvalidOperationException>(()=>world.Consequences(cure+1000));
            }
        }
        [Test]
        public void StormRootOnlyConsequencesHaveZeroSubject()
        {
            var fixture=DecodeFixture("consequences-storm-v9",Geometry());
            Assert.That(fixture.RootTag,Is.EqualTo(16));Assert.That(fixture.Subject,Is.Zero);Assert.That(Tags(fixture),Is.EqualTo(new uint[]{16,18,19}));
            Assert.That(fixture.End-fixture.Applied,Is.EqualTo(60000000));
            using(var world=new NativeSlice(71449,Start))
            {
                world.SetCamera(Vector3.zero,500);world.Steps(20);var frame=new SliceFrame();world.Fill(frame,out _);
                ulong root=world.Storm(60000000);var active=world.Consequences(root);
                Assert.That(active.RootTag,Is.EqualTo(16));Assert.That(active.Subject,Is.Zero);Assert.That(Tags(active),Is.EqualTo(new uint[]{16}));
                Assert.That(active.End-active.Applied,Is.EqualTo(60000000));Assert.That(active.Severity,Is.Zero);
                // One storm at a time, and no gathering under it; both are journalled rejections.
                Assert.Throws<InvalidOperationException>(()=>world.Storm(60000000));
                Assert.Throws<InvalidOperationException>(()=>world.Gather(frame.Buildings[0].Id,600000000));
                for(int i=0;i<7;i++) world.Steps(100);
                var ended=world.Consequences(root);Assert.That(Tags(ended),Is.EqualTo(new uint[]{16,18,19}));
                Assert.That(ended.Events[1].Time,Is.EqualTo(ended.End));Assert.That(ended.Events[2].A,Is.EqualTo(ended.Events[0].A));
                Assert.Throws<InvalidOperationException>(()=>world.Storm(0));
            }
        }
        [Test]
        public void GatheringConsequencesAllowRepeats()
        {
            var fixture=DecodeFixture("consequences-gathering-v9",Geometry());
            Assert.That(fixture.RootTag,Is.EqualTo(20));Assert.That(Tags(fixture),Is.EqualTo(new uint[]{20}));
            Assert.That(fixture.Subject,Is.EqualTo(fixture.Events[0].A));Assert.That(fixture.End-fixture.Applied,Is.EqualTo(600000000));
            // Monday 21:00 in New York, when most residents are home; a 1 km camera keeps everyone at L1.
            using(var world=new NativeSlice(71449,NativeWorld.UtcToSim(2026,6,23,1,0,0)))
            {
                var frame=new SliceFrame();world.SetCamera(Vector3.zero,1000);
                for(int i=0;i<100;i++) {world.Fill(frame,out _);if(frame.L1==(uint)frame.Population) break;world.Steps(1);}
                Assert.That(frame.L1,Is.EqualTo((uint)frame.Population));
                ulong target=frame.Buildings[0].Id;ulong root=world.Gather(target,1800000000);
                var called=world.Consequences(root);Assert.That(called.RootTag,Is.EqualTo(20));Assert.That(called.Subject,Is.EqualTo(target));
                Assert.That(called.Events[0].D,Is.EqualTo(7),"Seven eligible residents fill the gathering cap.");
                bool away=false,travelling=false;SliceConsequences c=called;
                for(int step=0;step<400&&Count(c,6)<7;step++)
                {
                    world.Steps(100);world.Fill(frame,out _);
                    // Every attendee inspection passes the schema3 kind3 rule on real native bytes.
                    for(int i=0;i<frame.Population;i++)
                    {
                        var r=world.Inspect(frame.Residents[i].Id);if(r.OverrideKind!=3) continue;
                        Assert.That(r.OverrideRoot,Is.EqualTo(root));
                        if(r.Location==2) travelling=true;
                        else if(r.From==target&&r.Home!=target) {away=true;Assert.That(r.Action,Is.EqualTo(3));Assert.That(r.Activity,Is.EqualTo(5));}
                    }
                    c=world.Consequences(root);
                }
                Assert.That(away,Is.True,"An attendee must be inspected at the gathering, away from home.");Assert.That(travelling,Is.True);
                Assert.That(Count(c,21),Is.EqualTo(7));Assert.That(Count(c,6),Is.EqualTo(7));Assert.That(Count(c,22),Is.EqualTo(1));
                Assert.That(c.Events.Length,Is.EqualTo(16));
                foreach(var e in c.Events) if(e.Tag==22) {Assert.That(e.B,Is.EqualTo(7));Assert.That(e.Time,Is.EqualTo(c.End));}
            }
        }
        [Test]
        public void AttendeesAfflictedMidGatheringInspectUntilHomeAndDecodeTheirIllness()
        {
            // ADR0038 §4 / Review Focus2: an illness cast on an attendee keeps the home→target
            // leg (walking out) or walks target→home (at the target); both are kind1 legs the
            // core publishes, and the illness group owns the ReturnedHome row.
            using(var world=new NativeSlice(71449,NativeWorld.UtcToSim(2026,6,23,1,0,0)))
            {
                var frame=new SliceFrame();world.SetCamera(Vector3.zero,1000);
                for(int i=0;i<100;i++) {world.Fill(frame,out _);if(frame.L1==(uint)frame.Population) break;world.Steps(1);}
                ulong target=frame.Buildings[0].Id;ulong gathering=world.Gather(target,1800000000);
                ulong walker=0,guest=0,walkerRoot=0,guestRoot=0;bool walkerHome=false,guestHome=false,walkerLeg=false,guestLeg=false;
                for(int step=0;step<4000&&!(walkerHome&&guestHome);step++)
                {
                    world.Steps(10);world.Fill(frame,out _);
                    for(int i=0;i<frame.Population;i++)
                    {
                        ulong id=frame.Residents[i].Id;if(id==walker||id==guest) continue;
                        var r=world.Inspect(id);if(r.OverrideKind!=3||r.Home==target) continue;
                        if(walker==0&&r.Location==2&&r.From==r.Home&&r.To==target) {walker=id;walkerRoot=world.Illness(id,0,5,3600000000).Consequences.Root;}
                        else if(guest==0&&r.Location==0&&r.From==target&&r.Action==3) {guest=id;guestRoot=world.Illness(id,0,5,3600000000).Consequences.Root;}
                    }
                    foreach(var (id,root) in new[]{(walker,walkerRoot),(guest,guestRoot)})
                    {
                        if(id==0) continue;
                        // Illness() inspects the resident and decodes its illness group on every step.
                        var sample=world.Illness(id,root);var r=sample.Inspection;
                        Assert.That(r.OverrideKind,Is.EqualTo(1));Assert.That(r.OverrideRoot,Is.EqualTo(root));
                        bool leg=r.Location==2&&r.FromExit==uint.MaxValue&&r.ToExit==uint.MaxValue;
                        bool home=r.Location==0&&r.From==r.Home;
                        bool returned=false;foreach(var e in sample.Consequences.Events) if(e.Tag==6) {returned=true;Assert.That(e.A,Is.EqualTo(id));}
                        if(id==walker) {walkerLeg|=leg;walkerHome|=home&&returned;} else {guestLeg|=leg;guestHome|=home&&returned;}
                    }
                }
                Assert.That(walker,Is.Not.Zero,"An attendee must be afflicted while walking out.");Assert.That(guest,Is.Not.Zero,"An attendee must be afflicted at the gathering.");
                Assert.That(walkerLeg&&guestLeg,Is.True,"Both must be inspected on a kind1 leg between featured buildings.");
                Assert.That(walkerHome&&guestHome,Is.True,"Both illness groups must record ReturnedHome.");
                // The gathering group lacks both interrupted attendees' ReturnedHome rows.
                foreach(var e in world.Consequences(gathering).Events) if(e.Tag==6) {Assert.That(e.A,Is.Not.EqualTo(walker));Assert.That(e.A,Is.Not.EqualTo(guest));}
            }
        }
        [Test]
        public void SliceHostSaveAndLoadIntegration()
        {
            string path=Path.Combine(Path.GetTempPath(),"unity_host_rt_"+Guid.NewGuid().ToString("N")+".wssave");
            try
            {
                var frameA=new SliceFrame();
                var frameB=new SliceFrame();
                using(var hostA=new SliceHost(71449,Start))
                {
                    hostA.SetScale(1);
                    System.Threading.Thread.Sleep(50);
                    hostA.Fill(frameA,out _);
                    hostA.Save(path);
                    Assert.That(File.Exists(path),Is.True);

                    using(var hostB=SliceHost.Load(path))
                    {
                        Assert.That(hostB.Population,Is.EqualTo(hostA.Population));
                        hostB.Fill(frameB,out _);
                        Assert.That(frameB.Population,Is.EqualTo(frameA.Population));
                    }
                }
            }
            finally
            {
                if(File.Exists(path)) File.Delete(path);
            }
        }
    }
}
