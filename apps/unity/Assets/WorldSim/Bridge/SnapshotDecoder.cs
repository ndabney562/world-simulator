using System;
using System.Runtime.InteropServices;
using Unity.Collections;
using UnityEngine;

namespace WorldSim.Bridge
{
    /// <summary>Alignment-independent little-endian ABI-v2 decoding; no managed allocation on success.</summary>
    public static class SnapshotDecoder
    {
        [StructLayout(LayoutKind.Explicit)]
        struct FloatBits { [FieldOffset(0)] public uint Bits; [FieldOffset(0)] public float Value; }
        static uint U32(NativeArray<byte> b, int o) => (uint)(b[o] | b[o+1]<<8 | b[o+2]<<16 | b[o+3]<<24);
        static ulong U64(NativeArray<byte> b, int o) => U32(b,o) | (ulong)U32(b,o+4)<<32;
        static float F32(NativeArray<byte> b, int o) => new FloatBits { Bits = U32(b,o) }.Value;
        static bool Finite(float f) => !float.IsNaN(f) && !float.IsInfinity(f);

        public static ulong Decode(NativeArray<byte> bytes, int length, Matrix4x4[] destination, int expectedCount)
        {
            if (!bytes.IsCreated || length < 24 || length > bytes.Length || destination == null || expectedCount < 0)
                throw new ArgumentException("Invalid snapshot storage.");
            NativeWorld.RequireVersion(U32(bytes,0));
            uint count = U32(bytes,16);
            if (U32(bytes,4) != 24 || U32(bytes,20) != 32 || count != expectedCount || count > destination.Length || 24UL + count * 32UL != (ulong)length)
                throw new ArgumentException("Invalid snapshot layout/count.");
            // Validate the entire frame before modifying the published destination.
            for (int i=0; i<expectedCount; i++)
            {
                int o=24+i*32;
                if (U64(bytes,o) != (ulong)i+1 || U32(bytes,o+8) != 0 || U32(bytes,o+28) != 0 ||
                    !Finite(F32(bytes,o+12)) || !Finite(F32(bytes,o+16)) || !Finite(F32(bytes,o+20)) || !Finite(F32(bytes,o+24)))
                    throw new ArgumentException("Invalid debug marker record.");
            }
            for (int i=0; i<expectedCount; i++)
            {
                int o=24+i*32; float heading=F32(bytes,o+24);
                var position=new Vector3(F32(bytes,o+12),F32(bytes,o+20),F32(bytes,o+16));
                var forward=new Vector3(Mathf.Cos(heading),0,Mathf.Sin(heading));
                destination[i]=Matrix4x4.TRS(position,Quaternion.LookRotation(forward,Vector3.up),Vector3.one);
            }
            return U64(bytes,8);
        }
    }
}
