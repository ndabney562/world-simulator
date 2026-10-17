using System;
using System.Runtime.InteropServices;
using Unity.Collections;
using UnityEngine;

namespace WorldSim.Bridge
{
    /// <summary>Alignment-independent little-endian ABI-v3 decoding (ADR 0022); no managed allocation on success.</summary>
    public static class SnapshotDecoder
    {
        public const uint KindBuilding = 1, KindResident = 2;
        /// <summary>Resident capsule: Unity's capsule primitive is 2 m tall, so this scale gives about 1.7 m.</summary>
        static readonly Vector3 ResidentScale = new Vector3(0.5f, 0.85f, 0.5f);
        [StructLayout(LayoutKind.Explicit)]
        struct FloatBits { [FieldOffset(0)] public uint Bits; [FieldOffset(0)] public float Value; }
        static uint U32(NativeArray<byte> b, int o) => (uint)(b[o] | b[o+1]<<8 | b[o+2]<<16 | b[o+3]<<24);
        static ulong U64(NativeArray<byte> b, int o) => U32(b,o) | (ulong)U32(b,o+4)<<32;
        static float F32(NativeArray<byte> b, int o) => new FloatBits { Bits = U32(b,o) }.Value;
        static bool Finite(float f) => !float.IsNaN(f) && !float.IsInfinity(f);

        /// <summary>
        /// Validates the whole frame, then fills <paramref name="frame"/>. A rejected frame leaves it untouched.
        /// Records must be buildings first, then residents, each kind with its own field rules.
        /// </summary>
        public static ulong Decode(NativeArray<byte> bytes, int length, RenderFrame frame)
        {
            if (!bytes.IsCreated || length < 24 || length > bytes.Length || frame == null)
                throw new ArgumentException("Invalid snapshot storage.");
            NativeWorld.RequireVersion(U32(bytes,0));
            uint count = U32(bytes,16);
            if (U32(bytes,4) != 24 || U32(bytes,20) != 32 || 24UL + count * 32UL != (ulong)length)
                throw new ArgumentException("Invalid snapshot layout/count.");
            int buildings = 0, residents = 0;
            for (int i=0; i<count; i++)
            {
                int o=24+i*32; uint kind=U32(bytes,o+8);
                float e=F32(bytes,o+12), n=F32(bytes,o+16), u=F32(bytes,o+20), a=F32(bytes,o+24), b=F32(bytes,o+28);
                if (!Finite(e)||!Finite(n)||!Finite(u)||!Finite(a)||!Finite(b)) throw new ArgumentException("Non-finite render record.");
                if (kind == KindBuilding)
                {
                    if (residents != 0 || a <= 0 || b <= 0) throw new ArgumentException("Invalid building record.");
                    buildings++;
                }
                else if (kind == KindResident)
                {
                    if (b != 0) throw new ArgumentException("Invalid resident record.");
                    residents++;
                }
                else throw new ArgumentException("Unknown render kind.");
            }
            if (buildings > frame.Buildings.Length || residents > frame.Residents.Length)
                throw new ArgumentException("Snapshot exceeds frame capacity.");
            for (int i=0; i<count; i++)
            {
                int o=24+i*32;
                float e=F32(bytes,o+12), n=F32(bytes,o+16), u=F32(bytes,o+20), a=F32(bytes,o+24), b=F32(bytes,o+28);
                if (i < buildings)
                    frame.Buildings[i]=Matrix4x4.TRS(new Vector3(e,u+b*0.5f,n),Quaternion.identity,new Vector3(a*2,b,a*2));
                else
                {
                    var forward=new Vector3(Mathf.Cos(a),0,Mathf.Sin(a)); // heading is east towards north; Unity x=east, z=north
                    frame.Residents[i-buildings]=Matrix4x4.TRS(new Vector3(e,u+ResidentScale.y,n),Quaternion.LookRotation(forward,Vector3.up),ResidentScale);
                }
            }
            frame.BuildingCount=buildings; frame.ResidentCount=residents; frame.Tick=U64(bytes,8);
            return frame.Tick;
        }
    }
}
