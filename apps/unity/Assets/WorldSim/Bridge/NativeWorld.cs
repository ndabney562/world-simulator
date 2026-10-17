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
        internal uint Count, Reserved;
    }

    internal sealed class WorldHandle : SafeHandleZeroOrMinusOneIsInvalid
    {
        internal WorldHandle(IntPtr pointer) : base(true) { SetHandle(pointer); }
        protected override bool ReleaseHandle() { NativeMethods.Destroy(handle); return true; }
    }

    internal static class NativeMethods
    {
#if UNITY_IOS && !UNITY_EDITOR
        const string Library = "__Internal";
#elif UNITY_EDITOR_OSX || UNITY_STANDALONE_OSX
        const string Library = "ws_ffi";
#else
        const string Library = "ws_ffi_unsupported_platform";
#endif
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "ws_abi_version")]
        internal static extern uint Version();
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "ws_create")]
        internal static extern IntPtr Create(in NativeConfig config);
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "ws_destroy")]
        internal static extern void Destroy(IntPtr handle);
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "ws_advance")]
        internal static extern uint Advance(WorldHandle handle, uint steps);
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
        public const uint AbiVersion = 1;
        readonly object gate = new object();
        WorldHandle handle;
        NativeArray<byte> bytes;
        bool disposed;
        public int MarkerCount { get; }

        internal static void RequireVersion(uint actual)
        {
            if (actual != AbiVersion) throw new InvalidOperationException($"Native ABI {actual}; expected {AbiVersion}. Restart Unity after rebuilding the plugin.");
        }
        public NativeWorld(ulong seed, int markerCount, long startMicroseconds = 0)
        {
            if (markerCount < 0 || markerCount > 100000) throw new ArgumentOutOfRangeException(nameof(markerCount));
            RequireVersion(NativeMethods.Version());
            var config = new NativeConfig { Version = AbiVersion, Size = 32, Seed = seed, Start = startMicroseconds, Count = (uint)markerCount };
            var pointer = NativeMethods.Create(in config);
            if (pointer == IntPtr.Zero) throw new InvalidOperationException(ReadError(IntPtr.Zero));
            handle = new WorldHandle(pointer);
            try { bytes = new NativeArray<byte>(checked(24 + markerCount * 32), Allocator.Persistent); }
            catch { handle.Dispose(); throw; }
            MarkerCount = markerCount;
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
        public void SubmitNoOp()
        {
            // Control path allocation only; never submitted from the frame loop.
            byte[] batch = {1,0,0,0,1,0,0,0,0,0,0,0,0,0,0,0};
            lock (gate) { RequireLive(); Check(NativeMethods.Submit(handle, batch, (UIntPtr)batch.Length)); }
        }
        /// <summary>One native fill, then complete decode into the reusable destination. No pointers escape.</summary>
        public unsafe ulong FillSnapshot(UnityEngine.Matrix4x4[] destination)
        {
            lock (gate)
            {
                RequireLive();
                // The buffer belongs to this object; lock excludes filling/disposing concurrently.
                IntPtr pointer = (IntPtr)NativeArrayUnsafeUtility.GetUnsafePtr(bytes);
                Check(NativeMethods.Fill(handle, pointer, (UIntPtr)bytes.Length, out var written));
                return SnapshotDecoder.Decode(bytes, checked((int)written.ToUInt64()), destination, MarkerCount);
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
