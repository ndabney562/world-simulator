using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
using Unity.Collections;
using Unity.Collections.LowLevel.Unsafe;
using UnityEngine;

namespace WorldSim.Bridge
{
    [StructLayout(LayoutKind.Sequential)] internal struct NativeSliceConfig
    {
        internal uint Version, Size;
        internal ulong Seed;
        internal long Start;
        internal ulong Reserved;
    }
    [StructLayout(LayoutKind.Sequential)] internal struct NativeSliceOperation
    {
        internal uint Version, Size, Tag, Reserved;
        internal ulong A, B, C, D, E, F;
    }
    internal sealed class SliceHandle : SafeHandleZeroOrMinusOneIsInvalid
    {
        internal SliceHandle(IntPtr pointer) : base(true) { SetHandle(pointer); }
        protected override bool ReleaseHandle() { NativeSliceMethods.Destroy(handle); return true; }
    }
    internal static class NativeSliceMethods
    {
        const string Library = NativeMethods.Library;
        [DllImport(Library, CallingConvention=CallingConvention.Cdecl, EntryPoint="ws_slice_create")]
        internal static extern IntPtr Create(in NativeSliceConfig config);
        [DllImport(Library, CallingConvention=CallingConvention.Cdecl, EntryPoint="ws_slice_destroy")]
        internal static extern void Destroy(IntPtr handle);
        [DllImport(Library, CallingConvention=CallingConvention.Cdecl, EntryPoint="ws_slice_apply")]
        internal static extern uint Apply(SliceHandle handle,in NativeSliceOperation operation);
        [DllImport(Library, CallingConvention=CallingConvention.Cdecl, EntryPoint="ws_slice_fill_geometry")]
        internal static extern uint Geometry(SliceHandle handle,IntPtr buffer,UIntPtr capacity,out UIntPtr written);
        [DllImport(Library, CallingConvention=CallingConvention.Cdecl, EntryPoint="ws_slice_fill_snapshot")]
        internal static extern uint Fill(SliceHandle handle,IntPtr buffer,UIntPtr capacity,out UIntPtr written);
        [DllImport(Library, CallingConvention=CallingConvention.Cdecl, EntryPoint="ws_slice_get_calendar_sky")]
        internal static extern uint Sky(SliceHandle handle,ref CalendarSky sky);
        [DllImport(Library, CallingConvention=CallingConvention.Cdecl, EntryPoint="ws_slice_inspect")]
        internal static extern uint Inspect(SliceHandle handle,ulong id,IntPtr buffer,UIntPtr capacity,out UIntPtr written);
        [DllImport(Library, CallingConvention=CallingConvention.Cdecl, EntryPoint="ws_slice_consequences")]
        internal static extern uint Consequences(SliceHandle handle,ulong root,IntPtr buffer,UIntPtr capacity,out UIntPtr written);
        [DllImport(Library, CallingConvention=CallingConvention.Cdecl, EntryPoint="ws_slice_last_error")]
        internal static extern UIntPtr Error(IntPtr handle,byte[] bytes,UIntPtr capacity);
    }
    /// <summary>Windowed actual native apply costs; includes host operations and controls.</summary>
    public struct SliceAdvanceStats
    {
        public double ApplyTotalMs, MaxApplyMs;
        public ulong CommittedTicks;
        public int Calls;
        public double MeanPerStepMs => CommittedTicks==0 ? 0 : ApplyTotalMs/CommittedTicks;
    }

    /// <summary>One serialized real-slice handle, immutable geometry and persistent bounded snapshot storage.</summary>
    public sealed class NativeSlice : IDisposable
    {
        readonly object gate=new object();
        SliceHandle handle;
        NativeArray<byte> bytes;
        NativeArray<byte> inspectionBytes;
        bool disposed;
        long applyTicks,maxApplyTicks;
        int applyCalls;
        ulong lastStatisticsTick;
        ulong[] residentIds;
        bool rosterCaptured;
        public SliceGeometry Geometry { get; private set; }
        public int Population { get; private set; }
        public NativeSlice(ulong seed,long startMicroseconds)
        {
            NativeWorld.RequireVersion(NativeMethods.Version()); // Before any new symbol call.
            var config=new NativeSliceConfig {Version=NativeWorld.AbiVersion,Size=32,Seed=seed,Start=startMicroseconds};
            IntPtr pointer=NativeSliceMethods.Create(in config);
            if(pointer==IntPtr.Zero) throw new InvalidOperationException(ReadError(IntPtr.Zero));
            handle=new SliceHandle(pointer);
            try
            {
                UIntPtr required;
                uint status=NativeSliceMethods.Geometry(handle,IntPtr.Zero,UIntPtr.Zero,out required);
                if(status!=3) Check(status);
                ulong length=required.ToUInt64();
                if(length<96 || length>96UL+256*48UL+32768*24UL+131072*4UL) throw new InvalidOperationException("Invalid native geometry capacity.");
                using(var temporary=new NativeArray<byte>((int)length,Allocator.Temp))
                {
                    unsafe
                    {
                        Check(NativeSliceMethods.Geometry(handle,(IntPtr)NativeArrayUnsafeUtility.GetUnsafePtr(temporary),(UIntPtr)temporary.Length,out required));
                    }
                    if(required.ToUInt64()!=(ulong)temporary.Length) throw new InvalidOperationException("Unstable native geometry size.");
                    Geometry=SliceDecoder.DecodeGeometry(temporary,temporary.Length);
                }
                status=NativeSliceMethods.Fill(handle,IntPtr.Zero,UIntPtr.Zero,out required);
                if(status!=3) Check(status);
                length=required.ToUInt64();
                if(length<128+4*48 || length>1760 || (length-128-4*48)%48!=0) throw new InvalidOperationException("Invalid native dynamic capacity.");
                Population=(int)((length-128-4*48)/48);
                if(Population<10 || Population>30) throw new InvalidOperationException("Invalid native cohort.");
                bytes=new NativeArray<byte>((int)length,Allocator.Persistent);
                residentIds=new ulong[Population];
                inspectionBytes=new NativeArray<byte>(8192,Allocator.Persistent);
            }
            catch
            {
                if(bytes.IsCreated) bytes.Dispose();if(inspectionBytes.IsCreated) inspectionBytes.Dispose();handle.Dispose();throw;
            }
        }
        static string ReadError(IntPtr pointer)
        {
            var buffer=new byte[256];NativeSliceMethods.Error(pointer,buffer,(UIntPtr)buffer.Length);
            int end=Array.IndexOf(buffer,(byte)0);
            return System.Text.Encoding.UTF8.GetString(buffer,0,end<0?buffer.Length:end);
        }
        void Check(uint status)
        {
            if(status!=0) throw new InvalidOperationException($"Native slice status{status}: {ReadError(handle.DangerousGetHandle())}");
        }
        void RequireLive() { if(disposed) throw new ObjectDisposedException(nameof(NativeSlice)); }
        static NativeSliceOperation Operation(uint tag) => new NativeSliceOperation {Version=NativeWorld.AbiVersion,Size=64,Tag=tag};
        void Apply(in NativeSliceOperation operation)
        {
            long before=Stopwatch.GetTimestamp();uint status=NativeSliceMethods.Apply(handle,in operation);
            long elapsed=Stopwatch.GetTimestamp()-before;applyTicks+=elapsed;maxApplyTicks=Math.Max(maxApplyTicks,elapsed);applyCalls++;
            Check(status);
        }
        public void Steps(uint count)
        {
            if(count>100) throw new ArgumentOutOfRangeException(nameof(count));
            lock(gate) {RequireLive();var operation=Operation(1);operation.A=count;Apply(in operation);}
        }
        public void SetScale(uint scale)
        {
            if(scale!=0&&scale!=1&&scale!=10&&scale!=100) throw new ArgumentOutOfRangeException(nameof(scale));
            lock(gate) {RequireLive();var operation=Operation(2);operation.A=scale;Apply(in operation);}
        }
        /// <summary>Core owns cap/drop/remainder and scale; the host sends only elapsed microseconds.</summary>
        public void HostElapsed(ulong microseconds)
        {
            lock(gate) {RequireLive();var operation=Operation(3);operation.A=microseconds;Apply(in operation);}
        }
        static ulong Bits(double value) => unchecked((ulong)BitConverter.DoubleToInt64Bits(value));
        /// <summary>One coarse Unity-space camera request. Core owns normalization and simulation detail.</summary>
        public void SetCamera(Vector3 target,double radiusMetres)
        {
            lock(gate)
            {
                RequireLive();var operation=Operation(4);operation.A=Bits(target.x);operation.B=Bits(target.z);operation.C=Bits(target.y);operation.D=Bits(radiusMetres);Apply(in operation);
            }
        }
        CalendarSky ReadSky()
        {
            var sky=new CalendarSky {Version=NativeWorld.AbiVersion,Size=160};
            Check(NativeSliceMethods.Sky(handle,ref sky));sky.Validate();return sky;
        }
        /// <summary>Pure diagnostic query; does not tick, flush or update the published frame.</summary>
        public CalendarSky GetCalendarSky() {lock(gate) {RequireLive();return ReadSky();}}
        /// <summary>Fill and sky query share one gate; all coherence checks precede managed publication.</summary>
        public unsafe ulong Fill(SliceFrame destination,out SliceAdvanceStats stats)
        {
            lock(gate)
            {
                RequireLive();UIntPtr written;
                Check(NativeSliceMethods.Fill(handle,(IntPtr)NativeArrayUnsafeUtility.GetUnsafePtr(bytes),(UIntPtr)bytes.Length,out written));
                ulong size=written.ToUInt64();if(size!=(ulong)bytes.Length || SliceDecoder.U32(bytes,48)!=(uint)Population || SliceDecoder.U32(bytes,52)!=(uint)Population) throw new InvalidOperationException("Native slice cohort/length changed.");
                var sky=ReadSky();
                if(sky.Tick!=SliceDecoder.U64(bytes,8)||sky.SimTimeMicroseconds!=unchecked((long)SliceDecoder.U64(bytes,16))||
                    sky.StateHash!=SliceDecoder.U64(bytes,24)||sky.ScaleTag!=SliceDecoder.U32(bytes,44)||sky.SiteDatasetVersion!=SliceDecoder.U64(bytes,32)||sky.Tick<lastStatisticsTick)
                    throw new InvalidOperationException("Incoherent native slice/sky pair.");
                if(rosterCaptured) for(int i=0;i<Population;i++)
                    if(SliceDecoder.U64(bytes,128+i*48)!=residentIds[i]) throw new InvalidOperationException("Native slice identity roster changed.");
                ulong tick=SliceDecoder.DecodeSnapshot(bytes,(int)size,Geometry,destination);
                if(!rosterCaptured) {for(int i=0;i<Population;i++) residentIds[i]=destination.Residents[i].Id;rosterCaptured=true;}
                destination.Sky=sky;
                double milliseconds=1000.0/Stopwatch.Frequency;
                stats=new SliceAdvanceStats {ApplyTotalMs=applyTicks*milliseconds,MaxApplyMs=maxApplyTicks*milliseconds,CommittedTicks=tick-lastStatisticsTick,Calls=applyCalls};
                applyTicks=0;maxApplyTicks=0;applyCalls=0;lastStatisticsTick=tick;
                return tick;
            }
        }
        /// <summary>One tap sample; no render-loop polling. Publish only a wholly validated native record.</summary>
        public unsafe SliceInspection Inspect(ulong id)
        {
            lock(gate)
            {
                RequireLive();
                Check(NativeSliceMethods.Inspect(handle,id,(IntPtr)NativeArrayUnsafeUtility.GetUnsafePtr(inspectionBytes),(UIntPtr)inspectionBytes.Length,out var written));
                ulong size=written.ToUInt64();if(size>8192) throw new InvalidOperationException("Oversized inspection.");
                var record=InspectionDecoder.Decode(inspectionBytes,(int)size,Geometry,id);
                var sky=ReadSky();
                if(record.Tick!=sky.Tick||record.Time!=sky.SimTimeMicroseconds||record.StateHash!=sky.StateHash||record.Dataset!=sky.SiteDatasetVersion||record.Scale!=sky.ScaleTag)
                    throw new InvalidOperationException("Incoherent inspection/sky pair.");
                record.Sky=sky;return record;
            }
        }
        /// <summary>Apply (root zero) or refresh a bounded consequence group under one native gate.</summary>
        public unsafe SliceIllnessSample Illness(ulong id,ulong root=0,uint severity=5,ulong durationMicroseconds=7200000000)
        {
            lock(gate)
            {
                RequireLive();
                if(root==0) {var op=Operation(5);op.A=id;op.B=severity;op.C=durationMicroseconds;Apply(in op);}
                var record=Inspect(id);
                if(record.Kind!=1) throw new ArgumentException("Illness requires a resident.");
                if(root==0) foreach(var e in record.Events) if(e.Tag==13) {root=e.Id;break;}
                if(root==0) throw new InvalidOperationException("Missing illness intervention root.");
                Check(NativeSliceMethods.Consequences(handle,root,(IntPtr)NativeArrayUnsafeUtility.GetUnsafePtr(inspectionBytes),(UIntPtr)inspectionBytes.Length,out var written));
                ulong size=written.ToUInt64();if(size>8192) throw new InvalidOperationException("Oversized consequences.");
                var consequences=ConsequencesDecoder.Decode(inspectionBytes,(int)size,Geometry,root);
                if(consequences.Subject!=id||consequences.Tick!=record.Tick||consequences.Time!=record.Time||consequences.StateHash!=record.StateHash||consequences.Dataset!=record.Dataset)
                    throw new InvalidOperationException("Incoherent inspection/consequences pair.");
                return new SliceIllnessSample {Inspection=record,Consequences=consequences};
            }
        }
        public void Dispose()
        {
            lock(gate)
            {
                if(disposed) return;disposed=true;
                try {handle.Dispose();} finally {if(bytes.IsCreated) bytes.Dispose();if(inspectionBytes.IsCreated) inspectionBytes.Dispose();}
            }
        }
    }
}
