using System;
using System.Runtime.InteropServices;

namespace WorldSim.Bridge
{
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
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "ws_utc_to_sim")]
        internal static extern uint UtcToSim(in NativeUtcLabel label, out long time);
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "ws_slice_last_error")]
        internal static extern UIntPtr Error(IntPtr handle, byte[] buffer, UIntPtr capacity);
    }

    /// <summary>ABI version check and pure UTC conversion shared by every native caller.</summary>
    public static class NativeWorld
    {
        public const uint AbiVersion = 11;

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
            if (status != 0) throw new InvalidOperationException($"Native status {status}: {ReadError()}");
            return time;
        }
        static string ReadError()
        {
            var error = new byte[256];
            NativeMethods.Error(IntPtr.Zero, error, (UIntPtr)error.Length);
            int end = Array.IndexOf(error, (byte)0);
            return System.Text.Encoding.UTF8.GetString(error, 0, end < 0 ? error.Length : end);
        }
    }
}
