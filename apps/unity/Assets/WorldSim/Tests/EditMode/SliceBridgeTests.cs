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
            using(var bytes=new NativeArray<byte>(Fixture("slice-geometry-v5"),Allocator.Temp)) return SliceDecoder.DecodeGeometry(bytes,bytes.Length);
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
            for(uint old=1;old<=4;old++)
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
            using(var fixture=new NativeArray<byte>(Fixture("slice-snapshot-v5"),Allocator.Temp)) SliceDecoder.DecodeSnapshot(fixture,fixture.Length,expectedGeometry,expected);
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
                byte[] skyBytes=Fixture("slice-sky-v5");var pinned=GCHandle.Alloc(skyBytes,GCHandleType.Pinned);
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
            byte[] original=Fixture("slice-geometry-v5");
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
            var geometry=Geometry();byte[] original=Fixture("slice-snapshot-v5");var frame=new SliceFrame();
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
    }
}
