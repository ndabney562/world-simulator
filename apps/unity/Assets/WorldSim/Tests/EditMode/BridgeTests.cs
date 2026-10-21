using System;
using System.IO;
using System.Runtime.InteropServices;
using NUnit.Framework;
using Unity.Collections;
using UnityEngine;
using WorldSim.Bridge;

namespace WorldSim.Tests.EditMode
{
    public sealed class BridgeTests
    {
        static byte[] Fixture(string name="snapshot-v5.hex")
        {
            string hex=File.ReadAllText(Path.Combine(Application.dataPath,"../../../core/crates/ws-ffi/tests/fixtures/"+name)).Trim();
            var bytes=new byte[hex.Length/2];
            for(int i=0;i<bytes.Length;i++) bytes[i]=Convert.ToByte(hex.Substring(i*2,2),16);
            return bytes;
        }
        [Test]
        public void NativeConfigLayoutMatchesHeader()
        {
            Assert.That(Marshal.SizeOf<NativeConfig>(),Is.EqualTo(32));
            Assert.That(Marshal.OffsetOf<NativeConfig>("Seed").ToInt32(),Is.EqualTo(8));
            Assert.That(Marshal.OffsetOf<NativeConfig>("Start").ToInt32(),Is.EqualTo(16));
            Assert.That(Marshal.OffsetOf<NativeConfig>("ResidentsPerBuilding").ToInt32(),Is.EqualTo(24));
            Assert.Throws<InvalidOperationException>(()=>NativeWorld.RequireVersion(1));
        }
        [Test]
        public void NativeControlStateLayoutMatchesHeaderAndOldClientIsRefused()
        {
            Assert.That(Marshal.SizeOf<NativeControlState>(),Is.EqualTo(16));
            Assert.That(Marshal.OffsetOf<NativeControlState>("Version").ToInt32(),Is.Zero);
            Assert.That(Marshal.OffsetOf<NativeControlState>("Size").ToInt32(),Is.EqualTo(4));
            Assert.That(Marshal.OffsetOf<NativeControlState>("Scale").ToInt32(),Is.EqualTo(8));
            Assert.That(Marshal.OffsetOf<NativeControlState>("Reserved").ToInt32(),Is.EqualTo(12));
            Assert.That(NativeMethods.Version(),Is.EqualTo(5));
            var oldConfig=new NativeConfig { Version=2,Size=32,Seed=7,ResidentsPerBuilding=1 };
            var pointer=NativeMethods.Create(in oldConfig);
            if(pointer!=IntPtr.Zero) NativeMethods.Destroy(pointer);
            Assert.That(pointer,Is.EqualTo(IntPtr.Zero));
            foreach(string name in new[]{"snapshot-v1.hex","snapshot-v2.hex","snapshot-v3.hex","snapshot-v4.hex"})
                using(var bytes=new NativeArray<byte>(Fixture(name),Allocator.Temp))
                    Assert.Throws<InvalidOperationException>(()=>SnapshotDecoder.Decode(bytes,bytes.Length,new RenderFrame(5,5)));
        }
        [Test]
        public void CommandsStayPendingUntilFlushAndResumeAtTheSameTick()
        {
            using(var world=new NativeWorld(7,1))
            {
                var transforms=new RenderFrame(5,5);
                Assert.That(world.CommittedScale,Is.EqualTo(1));
                world.SubmitTimeScale(0);
                Assert.That(world.CommittedScale,Is.EqualTo(1));
                Assert.That(world.FlushCommands(),Is.Zero);
                Assert.That(world.FillSnapshot(transforms),Is.Zero);
                world.SubmitNoOp();
                world.SubmitTimeScale(10);
                Assert.That(world.CommittedScale,Is.Zero);
                Assert.That(world.FlushCommands(),Is.EqualTo(10));
                Assert.That(world.FillSnapshot(transforms),Is.Zero);
                world.SubmitTimeScale(0);world.FlushCommands();
                world.Advance(2); // Explicit native ticks remain independent of pacing scale.
                Assert.That(world.FillSnapshot(transforms),Is.EqualTo(2));
                Assert.Throws<ArgumentOutOfRangeException>(()=>world.SubmitTimeScale(2));
                Assert.Throws<InvalidOperationException>(()=>world.SubmitCommands(new byte[]{1,0,0,0,0,0,0,0}));
                Assert.That(world.CommittedScale,Is.Zero);
                world.Dispose();
                Assert.Throws<ObjectDisposedException>(()=>world.FlushCommands());
                Assert.Throws<ObjectDisposedException>(()=>{ var ignored=world.CommittedScale; });
            }
        }
        static byte[] Synthetic(float heading=Mathf.PI/2)
        {
            var b=new byte[24+2*32];
            void U32(int o,uint v){ Array.Copy(BitConverter.GetBytes(v),0,b,o,4); }
            void F32(int o,float v){ Array.Copy(BitConverter.GetBytes(v),0,b,o,4); }
            U32(0,5);U32(4,24);Array.Copy(BitConverter.GetBytes(7UL),0,b,8,8);U32(16,2);U32(20,32);
            U32(24,1);U32(32,1);F32(36,1);F32(40,2);F32(44,3);F32(48,5);F32(52,8); // building
            U32(56,2);U32(64,2);F32(68,4);F32(72,6);F32(76,0);F32(80,heading);F32(84,0); // resident, heading east towards north (default north)
            return b;
        }
        [Test]
        public void SharedFixtureDecodesBuildingsThenResidents()
        {
            using(var bytes=new NativeArray<byte>(Fixture(),Allocator.Temp))
            {
                var frame=new RenderFrame(5,5);
                Assert.That(SnapshotDecoder.Decode(bytes,bytes.Length,frame),Is.EqualTo(0));
                Assert.That(frame.BuildingCount,Is.EqualTo(5));
                Assert.That(frame.ResidentCount,Is.EqualTo(5));
                // The first building is the snapshot origin: half extent 11 m, height 21 m, centred on its base.
                Vector3 centre=frame.Buildings[0].GetColumn(3);
                Assert.That(Vector3.Distance(centre,new Vector3(0,10.5f,0)),Is.LessThan(0.00001f));
                Assert.That(frame.Buildings[0].lossyScale.x,Is.EqualTo(22f).Within(0.0001f));
                Assert.That(frame.Buildings[0].lossyScale.y,Is.EqualTo(21f).Within(0.0001f));
                Assert.Throws<ArgumentException>(()=>SnapshotDecoder.Decode(bytes,bytes.Length-1,frame));
                Assert.Throws<ArgumentException>(()=>SnapshotDecoder.Decode(bytes,bytes.Length,new RenderFrame(5,4)));
            }
        }
        // North is the identity rotation, so the other headings prove the decoder actually applies heading.
        [TestCase(Mathf.PI/2,0f,1f)]  // north
        [TestCase(0f,1f,0f)]          // east
        [TestCase(Mathf.PI,-1f,0f)]   // west
        [TestCase(-Mathf.PI/2,0f,-1f)]// south
        public void ToyEntityKindsDecodeToBoxesAndCapsulesWithEnuAxes(float heading,float east,float north)
        {
            using(var bytes=new NativeArray<byte>(Synthetic(heading),Allocator.Temp))
            {
                var frame=new RenderFrame(1,1);
                Assert.That(SnapshotDecoder.Decode(bytes,bytes.Length,frame),Is.EqualTo(7));
                Assert.That(frame.BuildingCount,Is.EqualTo(1));Assert.That(frame.ResidentCount,Is.EqualTo(1));
                // East to x, north to z, up to y; the box sits on its base: y = up + height / 2.
                Assert.That(Vector3.Distance(frame.Buildings[0].GetColumn(3),new Vector3(1,7,2)),Is.LessThan(0.00001f));
                Assert.That(Vector3.Distance(frame.Buildings[0].lossyScale,new Vector3(10,8,10)),Is.LessThan(0.0001f));
                Assert.That(Vector3.Distance(frame.Residents[0].GetColumn(3),new Vector3(4,0.85f,6)),Is.LessThan(0.00001f));
                // MultiplyVector includes the capsule's deliberate 0.5 forward scale; test heading and size independently.
                var forward = frame.Residents[0].MultiplyVector(Vector3.forward);
                Assert.That(Vector3.Distance(forward.normalized,new Vector3(east,0,north)),Is.LessThan(0.00001f));
                Assert.That(Vector3.Distance(frame.Residents[0].lossyScale,new Vector3(0.5f,0.85f,0.5f)),Is.LessThan(0.00001f));
            }
        }
        [TestCase(0,2)]   // ABI mismatch
        [TestCase(4,1)]   // header size
        [TestCase(16,2)]  // count disagrees with the length
        [TestCase(20,1)]  // record size
        [TestCase(32,7)]  // unknown kind
        [TestCase(32,2)]  // resident before the remaining buildings
        [TestCase(212,1)] // resident field b must be zero
        public void CorruptFrameDoesNotPublishTransforms(int offset,int value)
        {
            var source=Fixture();source[offset]=(byte)value;
            using(var bytes=new NativeArray<byte>(source,Allocator.Temp))
            {
                var frame=new RenderFrame(5,5);
                Assert.Catch(()=>SnapshotDecoder.Decode(bytes,bytes.Length,frame));
                Assert.That(frame.BuildingCount,Is.Zero);
                Assert.That(frame.Buildings[0],Is.EqualTo(default(Matrix4x4)));
            }
        }
        [Test]
        public void NonfiniteAndNonpositiveRecordsAreRejected()
        {
            var source=Fixture(); source[48]=0;source[49]=0;source[50]=128;source[51]=127; // building half extent NaN-ish infinity
            using(var bytes=new NativeArray<byte>(source,Allocator.Temp))
                Assert.Throws<ArgumentException>(()=>SnapshotDecoder.Decode(bytes,bytes.Length,new RenderFrame(5,5)));
            source=Fixture(); source[48]=0;source[49]=0;source[50]=0;source[51]=0; // half extent zero
            using(var bytes=new NativeArray<byte>(source,Allocator.Temp))
                Assert.Throws<ArgumentException>(()=>SnapshotDecoder.Decode(bytes,bytes.Length,new RenderFrame(5,5)));
        }
        [Test]
        public void WarmedFlushAndControlReadHaveNoManagedAllocations()
        {
            using(var world=new NativeWorld(7,1))
            {
                for(int i=0;i<10;i++) world.FlushCommands();
                long before=GC.GetAllocatedBytesForCurrentThread();
                uint scale=0;
                for(int i=0;i<100;i++) scale=world.FlushCommands();
                long bytes=GC.GetAllocatedBytesForCurrentThread()-before;
                Assert.That(scale,Is.EqualTo(1));
                Assert.That(bytes,Is.Zero,"Per-frame flush/control state must reuse storage.");
            }
        }
        [Test]
        public void WarmedFillAndDecodeHaveNoManagedAllocations()
        {
            using(var world=new NativeWorld(7,200))
            {
                var matrices=new RenderFrame(5,1000);
                for(int i=0;i<10;i++) world.FillSnapshot(matrices);
                long before=GC.GetAllocatedBytesForCurrentThread();
                for(int i=0;i<100;i++) world.FillSnapshot(matrices);
                long bytes=GC.GetAllocatedBytesForCurrentThread()-before;
                Assert.That(bytes,Is.Zero,"Native fill and decoding must reuse storage.");
            }
        }
        [Test]
        public void MacPluginCreatesAdvancesAndDecodesSnapshots()
        {
            using(var world=new NativeWorld(7,200))
            {
                var transforms=new RenderFrame(5,1000);
                Assert.That(world.FillSnapshot(transforms),Is.EqualTo(0));
                Assert.That(transforms.BuildingCount,Is.EqualTo(5));Assert.That(transforms.ResidentCount,Is.EqualTo(1000));
                float before=0;for(int i=0;i<1000;i++) before+=transforms.Residents[i].m03+transforms.Residents[i].m23;
                world.SubmitNoOp(); world.Advance(30);
                Assert.That(world.FillSnapshot(transforms),Is.EqualTo(30));
                float after=0;for(int i=0;i<1000;i++) after+=transforms.Residents[i].m03+transforms.Residents[i].m23;
                Assert.That(after,Is.Not.EqualTo(before)); // someone moved
                Assert.Throws<InvalidOperationException>(()=>world.Advance(101));
                world.Dispose();world.Dispose();
                Assert.Throws<ObjectDisposedException>(()=>world.Advance(1));
            }
            Assert.Throws<InvalidOperationException>(()=>new NativeWorld(7,1,1));
        }
    }
}
