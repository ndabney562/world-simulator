using System;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
using Unity.Collections;
using Unity.Collections.LowLevel.Unsafe;

namespace WorldSim.Bridge
{
    [StructLayout(LayoutKind.Sequential)]
    internal struct NativeConfig
    {
        internal uint Version, Size;
        internal ulong Seed;
        internal long Start;
        internal uint ResidentsPerBuilding, Reserved;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct NativeControlState
    {
        internal uint Version, Size, Scale, Reserved;
    }

    internal sealed class WorldHandle : SafeHandleZeroOrMinusOneIsInvalid
    {
        internal WorldHandle(IntPtr pointer) : base(true) { SetHandle(pointer); }
        protected override bool ReleaseHandle() { NativeMethods.Destroy(handle); return true; }
    }

    internal static class NativeMethods
    {
#if (UNITY_IOS || UNITY_WEBGL) && !UNITY_EDITOR
        internal const string Library = "__Internal";
#elif UNITY_EDITOR_OSX || UNITY_STANDALONE_OSX
        internal const string Library = "ws_ffi";
#else
        internal const string Library = "ws_ffi_unsupported_platform";
#endif
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "ws_abi_version")]
        internal static extern uint Version();
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "ws_create")]
        internal static extern IntPtr Create(in NativeConfig config);
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "ws_destroy")]
        internal static extern void Destroy(IntPtr handle);
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "ws_advance")]
        internal static extern uint Advance(WorldHandle handle, uint steps);
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "ws_get_control_state")]
        internal static extern uint ControlState(WorldHandle handle, ref NativeControlState state);
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "ws_utc_to_sim")]
        internal static extern uint UtcToSim(in NativeUtcLabel label, out long time);
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "ws_get_calendar_sky")]
        internal static extern uint CalendarSky(WorldHandle handle, ref CalendarSky sky);
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "ws_submit")]
        internal static extern uint Submit(WorldHandle handle, byte[] bytes, UIntPtr length);
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "ws_fill_render_snapshot")]
        internal static extern uint Fill(WorldHandle handle, IntPtr buffer, UIntPtr capacity, out UIntPtr written);
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "ws_last_error")]
        internal static extern UIntPtr Error(IntPtr handle, byte[] buffer, UIntPtr capacity);
    }

    /// <summary>Owns one serialized native world and a persistent caller-owned snapshot buffer.</summary>
    public sealed class NativeWorld : IDisposable
    {
        public const uint AbiVersion = 9;
        /// <summary>The toy world has five buildings (ADR 0022).</summary>
        public const int BuildingCount = 5;
        public const int MaxResidentsPerBuilding = 2000;
        readonly object gate = new object();
        WorldHandle handle;
        NativeArray<byte> bytes;
        bool disposed;
        public int ResidentsPerBuilding { get; }
        public int ResidentCount => ResidentsPerBuilding * BuildingCount;
        public int InstanceCount => BuildingCount + ResidentCount;

        internal static void RequireVersion(uint actual)
        {
            if (actual != AbiVersion) throw new InvalidOperationException($"Native ABI {actual}; expected {AbiVersion}. Restart Unity after rebuilding the plugin.");
        }
        /// <summary>Pure core UTC/leap conversion, without grid rounding. No world is created or advanced.</summary>
        public static long UtcToSim(int year, uint month, uint day, uint hour, uint minute, uint second, uint microsecond = 0)
        {
            RequireVersion(NativeMethods.Version()); // refuse stale libraries before calling their missing symbol
            var label = new NativeUtcLabel { Version = AbiVersion, Size = 40, Year = year, Month = month, Day = day, Hour = hour, Minute = minute, Second = second, Microsecond = microsecond };
            uint status = NativeMethods.UtcToSim(in label, out long time);
            if (status != 0) throw new InvalidOperationException($"Native status {status}: {ReadError(IntPtr.Zero)}");
            return time;
        }
        public NativeWorld(ulong seed, int residentsPerBuilding, long startMicroseconds = 0)
        {
            if (residentsPerBuilding < 1 || residentsPerBuilding > MaxResidentsPerBuilding) throw new ArgumentOutOfRangeException(nameof(residentsPerBuilding));
            RequireVersion(NativeMethods.Version());
            var config = new NativeConfig { Version = AbiVersion, Size = 32, Seed = seed, Start = startMicroseconds, ResidentsPerBuilding = (uint)residentsPerBuilding };
            var pointer = NativeMethods.Create(in config);
            if (pointer == IntPtr.Zero) throw new InvalidOperationException(ReadError(IntPtr.Zero));
            handle = new WorldHandle(pointer);
            try { bytes = new NativeArray<byte>(checked(24 + (BuildingCount + BuildingCount * residentsPerBuilding) * 32), Allocator.Persistent); }
            catch { handle.Dispose(); throw; }
            ResidentsPerBuilding = residentsPerBuilding;
        }
        static string ReadError(IntPtr pointer)
        {
            var error = new byte[256];
            NativeMethods.Error(pointer, error, (UIntPtr)error.Length);
            int end = Array.IndexOf(error, (byte)0);
            return System.Text.Encoding.UTF8.GetString(error, 0, end < 0 ? error.Length : end);
        }
        void Check(uint status)
        {
            if (status != 0) throw new InvalidOperationException($"Native status {status}: {ReadError(handle.DangerousGetHandle())}");
        }
        void RequireLive() { if (disposed) throw new ObjectDisposedException(nameof(NativeWorld)); }
        public void Advance(uint steps)
        {
            lock (gate) { RequireLive(); Check(NativeMethods.Advance(handle, steps)); }
        }
        static bool ValidScale(uint scale) => scale==0 || scale==1 || scale==10 || scale==100;
        uint ReadControlState()
        {
            var state=new NativeControlState { Version=AbiVersion, Size=16 };
            Check(NativeMethods.ControlState(handle,ref state));
            RequireVersion(state.Version);
            if(state.Size!=16 || state.Reserved!=0 || !ValidScale(state.Scale))
                throw new InvalidOperationException("Invalid native control state.");
            return state.Scale;
        }
        /// <summary>Committed native scale; reading does not flush pending commands.</summary>
        public uint CommittedScale { get { lock(gate) { RequireLive(); return ReadControlState(); } } }
        /// <summary>Flush queued commands even while paused; return the committed scale without ticking.</summary>
        public uint FlushCommands()
        {
            lock(gate) { RequireLive(); Check(NativeMethods.Advance(handle,0)); return ReadControlState(); }
        }
        /// <summary>Copy a canonical v2 command batch (unchanged in ABI5) into the native queue; success means pending.</summary>
        public void SubmitCommands(byte[] batch)
        {
            if(batch==null) throw new ArgumentNullException(nameof(batch));
            lock(gate) { RequireLive(); Check(NativeMethods.Submit(handle,batch,(UIntPtr)batch.Length)); }
        }
        static void WriteU32(byte[] bytes,int offset,uint value)
        {
            for(int i=0;i<4;i++) bytes[offset+i]=(byte)(value>>(i*8));
        }
        static byte[] CommandBatch(uint opcode,int payloadSize)
        {
            // Control-path allocation only; no batch is created by the frame loop.
            var batch=new byte[16+payloadSize];
            WriteU32(batch,0,2); WriteU32(batch,4,1);
            WriteU32(batch,8,opcode); WriteU32(batch,12,(uint)payloadSize);
            WriteU32(batch,16,1); WriteU32(batch,20,0); // schema 1, Player provenance
            return batch;
        }
        public void SubmitNoOp() { SubmitCommands(CommandBatch(0,8)); }
        public void SubmitTimeScale(uint scale)
        {
            if(!ValidScale(scale)) throw new ArgumentOutOfRangeException(nameof(scale));
            var batch=CommandBatch(1,12); WriteU32(batch,24,scale); SubmitCommands(batch);
        }
        CalendarSky ReadSky()
        {
            var sky = new CalendarSky { Version = AbiVersion, Size = 160 };
            Check(NativeMethods.CalendarSky(handle, ref sky));
            sky.Validate();
            return sky;
        }
        /// <summary>Read the committed boundary without flushing pending commands or advancing.</summary>
        public CalendarSky GetCalendarSky()
        {
            lock (gate) { RequireLive(); return ReadSky(); }
        }
        /// <summary>One native fill, then complete decode into the reusable destination. No pointers escape.</summary>
        public unsafe ulong FillSnapshot(RenderFrame destination)
        {
            lock (gate)
            {
                RequireLive();
                // The buffer belongs to this object; lock excludes filling/disposing concurrently.
                IntPtr pointer = (IntPtr)NativeArrayUnsafeUtility.GetUnsafePtr(bytes);
                Check(NativeMethods.Fill(handle, pointer, (UIntPtr)bytes.Length, out var written));
                var sky = ReadSky();
                if (written.ToUInt64() < 24) throw new InvalidOperationException("Truncated native snapshot.");
                ulong tick = 0;
                for (int i = 0; i < 8; i++) tick |= (ulong)bytes[8 + i] << (i * 8);
                if (tick != sky.Tick) throw new InvalidOperationException("Calendar/render boundary mismatch.");
                SnapshotDecoder.Decode(bytes, checked((int)written.ToUInt64()), destination);
                destination.Sky = sky;
                return tick;
            }
        }
        public void Dispose()
        {
            lock (gate)
            {
                if (disposed) return;
                disposed = true;
                if (bytes.IsCreated) bytes.Dispose();
                handle.Dispose();
            }
        }
    }
}
