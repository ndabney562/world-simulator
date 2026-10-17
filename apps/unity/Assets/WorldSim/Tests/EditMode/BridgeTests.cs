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
        static byte[] Fixture()
        {
            string hex=File.ReadAllText(Path.Combine(Application.dataPath,"../../../core/crates/ws-ffi/tests/fixtures/snapshot-v2.hex")).Trim();
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
            Assert.That(Marshal.OffsetOf<NativeConfig>("Count").ToInt32(),Is.EqualTo(24));
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
            Assert.That(NativeMethods.Version(),Is.EqualTo(2));
            var oldConfig=new NativeConfig { Version=1,Size=32,Seed=7 };
            var pointer=NativeMethods.Create(in oldConfig);
            if(pointer!=IntPtr.Zero) NativeMethods.Destroy(pointer);
            Assert.That(pointer,Is.EqualTo(IntPtr.Zero));
            string legacy=File.ReadAllText(Path.Combine(Application.dataPath,"../../../core/crates/ws-ffi/tests/fixtures/snapshot-v1.hex")).Trim();
            var oldBytes=new byte[legacy.Length/2];
            for(int i=0;i<oldBytes.Length;i++) oldBytes[i]=Convert.ToByte(legacy.Substring(i*2,2),16);
            using(var bytes=new NativeArray<byte>(oldBytes,Allocator.Temp))
                Assert.Throws<InvalidOperationException>(()=>SnapshotDecoder.Decode(bytes,bytes.Length,new Matrix4x4[1],1));
        }
        [Test]
        public void CommandsStayPendingUntilFlushAndResumeAtTheSameTick()
        {
            using(var world=new NativeWorld(7,0))
            {
                var transforms=new Matrix4x4[0];
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
        [Test]
        public void SharedFixtureDecodesEnuAndHeading()
        {
            using(var bytes=new NativeArray<byte>(Fixture(),Allocator.Temp))
            {
                var transforms=new Matrix4x4[1];
                Assert.That(SnapshotDecoder.Decode(bytes,bytes.Length,transforms,1),Is.EqualTo(0));
                Assert.That(transforms[0].GetColumn(3).x,Is.EqualTo(0.14f));
                Assert.That(transforms[0].GetColumn(3).y,Is.EqualTo(0.5f));
                Assert.That(transforms[0].GetColumn(3).z,Is.EqualTo(0));
                Assert.That(Vector3.Distance(transforms[0].MultiplyVector(Vector3.forward),Vector3.right),Is.LessThan(0.00001f));
                Assert.Throws<ArgumentException>(()=>SnapshotDecoder.Decode(bytes,bytes.Length-1,transforms,1));
            }
        }
        [Test]
        public void EnuAxesAndNorthHeadingConvertOnce()
        {
            var source=Fixture();
            Array.Copy(BitConverter.GetBytes(1f),0,source,36,4);
            Array.Copy(BitConverter.GetBytes(2f),0,source,40,4);
            Array.Copy(BitConverter.GetBytes(3f),0,source,44,4);
            Array.Copy(BitConverter.GetBytes(Mathf.PI/2),0,source,48,4);
            using(var bytes=new NativeArray<byte>(source,Allocator.Temp))
            {
                var transforms=new Matrix4x4[1];
                SnapshotDecoder.Decode(bytes,bytes.Length,transforms,1);
                Assert.That(Vector3.Distance(transforms[0].GetColumn(3),new Vector3(1,3,2)),Is.LessThan(0.00001f));
                Assert.That(Vector3.Distance(transforms[0].MultiplyVector(Vector3.forward),Vector3.forward),Is.LessThan(0.00001f));
            }
        }
        [TestCase(0,1)] // ABI mismatch
        [TestCase(4,1)] // header size
        [TestCase(16,2)] // count
        [TestCase(20,1)] // record size
        [TestCase(24,2)] // ID
        [TestCase(32,1)] // unsupported kind
        [TestCase(52,1)] // reserved
        public void CorruptFrameDoesNotPublishTransforms(int offset,int value)
        {
            var source=Fixture();source[offset]=(byte)value;
            using(var bytes=new NativeArray<byte>(source,Allocator.Temp))
            {
                var transforms=new[]{Matrix4x4.zero};
                Assert.Catch(()=>SnapshotDecoder.Decode(bytes,bytes.Length,transforms,1));
                Assert.That(transforms[0],Is.EqualTo(Matrix4x4.zero));
            }
        }
        [Test]
        public void NonfiniteTransformIsRejected()
        {
            var source=Fixture(); source[36]=0;source[37]=0;source[38]=128;source[39]=127;
            using(var bytes=new NativeArray<byte>(source,Allocator.Temp))
            {
                Assert.Throws<ArgumentException>(()=>SnapshotDecoder.Decode(bytes,bytes.Length,new Matrix4x4[1],1));
            }
        }
        [Test]
        public void WarmedFlushAndControlReadHaveNoManagedAllocations()
        {
            using(var world=new NativeWorld(7,0))
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
            using(var world=new NativeWorld(7,1000))
            {
                var matrices=new Matrix4x4[1000];
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
            using(var world=new NativeWorld(7,1000))
            {
                var transforms=new Matrix4x4[1000];
                Assert.That(world.FillSnapshot(transforms),Is.EqualTo(0));
                float x=transforms[0].m03;
                world.SubmitNoOp(); world.Advance(3);
                Assert.That(world.FillSnapshot(transforms),Is.EqualTo(3));
                Assert.That(transforms[0].m03,Is.Not.EqualTo(x));
                Assert.Throws<InvalidOperationException>(()=>world.Advance(101));
                world.Dispose();world.Dispose();
                Assert.Throws<ObjectDisposedException>(()=>world.Advance(1));
            }
            Assert.Throws<InvalidOperationException>(()=>new NativeWorld(7,1,1));
        }
    }
}
