using System;
using System.Runtime.InteropServices;
using Unity.Collections;
using UnityEngine;

namespace WorldSim.Bridge
{
    /// <summary>Bounded explicit little-endian ABI5 slice decoding; warmed dynamic success allocates nothing.</summary>
    public static class SliceDecoder
    {
        public const int GeometryHeader = 96, SnapshotHeader = 128, Row = 48;
        [StructLayout(LayoutKind.Explicit)] struct FloatBits { [FieldOffset(0)] public uint Bits; [FieldOffset(0)] public float Value; }
        [StructLayout(LayoutKind.Explicit)] struct DoubleBits { [FieldOffset(0)] public ulong Bits; [FieldOffset(0)] public double Value; }
        internal static uint U32(NativeArray<byte> b, int o) => (uint)(b[o] | b[o+1]<<8 | b[o+2]<<16 | b[o+3]<<24);
        internal static ulong U64(NativeArray<byte> b, int o) => U32(b,o) | (ulong)U32(b,o+4)<<32;
        static float F32(NativeArray<byte> b, int o) => new FloatBits { Bits = U32(b,o) }.Value;
        static double F64(NativeArray<byte> b, int o) => new DoubleBits { Bits = U64(b,o) }.Value;
        static bool Finite(double f) => !double.IsNaN(f) && !double.IsInfinity(f);
        static bool ValidScale(uint scale) => scale == 0 || scale == 1 || scale == 10 || scale == 100;
        static Vector3 UnityVector(NativeArray<byte> bytes, int o) => new Vector3(F32(bytes,o),F32(bytes,o+8),F32(bytes,o+4));
        static bool Finite(Vector3 v) => Finite(v.x) && Finite(v.y) && Finite(v.z);
        static void RequireStorage(NativeArray<byte> bytes, int length, int minimum)
        {
            if (!bytes.IsCreated || length < minimum || length > bytes.Length) throw new ArgumentException("Invalid slice storage.");
            NativeWorld.RequireVersion(U32(bytes,0));
        }

        /// <summary>Validate once, then allocate a complete geometry cache. No object escapes on rejection.</summary>
        public static SliceGeometry DecodeGeometry(NativeArray<byte> bytes, int length)
        {
            RequireStorage(bytes,length,GeometryHeader);
            uint buildings=U32(bytes,20), vertices=U32(bytes,24), indices=U32(bytes,28);
            if (U32(bytes,4)!=96 || U64(bytes,8)!=SliceGeometry.DatasetVersion || U32(bytes,16)!=1 ||
                buildings<4 || buildings>256 || vertices==0 || vertices>32768 || indices==0 || indices>131072 || indices%3!=0 ||
                U32(bytes,32)!=48 || U32(bytes,36)!=24 || U32(bytes,40)!=4 || U32(bytes,44)!=0 ||
                96UL+buildings*48UL+vertices*24UL+indices*4UL!=(ulong)length) throw new ArgumentException("Invalid slice geometry layout/count.");
            double latitude=F64(bytes,48), longitude=F64(bytes,56), height=F64(bytes,64);
            if (!Finite(latitude)||!Finite(longitude)||!Finite(height) || Math.Abs(latitude-40.622079256973)>1e-9 ||
                Math.Abs(longitude+74.030160593242)>1e-9 || height!=0) throw new ArgumentException("Invalid slice frame origin.");
            Vector3 minimum=UnityVector(bytes,72), maximum=UnityVector(bytes,84);
            if (!Finite(minimum)||!Finite(maximum)||minimum.x>maximum.x||minimum.y>maximum.y||minimum.z>maximum.z ||
                minimum.magnitude>1000000 || maximum.magnitude>1000000) throw new ArgumentException("Invalid slice bounds.");
            var rows=new SliceBuilding[(int)buildings]; var featured=new ulong[4]; int featuredCount=0;
            uint vertexEnd=0,indexEnd=0; ulong lastBin=0;
            int vertexAt=checked(96+(int)buildings*48),indexAt=checked(vertexAt+(int)vertices*24);
            for (int i=0;i<rows.Length;i++)
            {
                int at=96+i*48; ulong bin=U64(bytes,at),id=U64(bytes,at+8); uint flags=U32(bytes,at+36);
                uint firstVertex=U32(bytes,at+16),vertexCount=U32(bytes,at+20),firstIndex=U32(bytes,at+24),indexCount=U32(bytes,at+28);
                float sourceHeight=F32(bytes,at+32),east=F32(bytes,at+40),north=F32(bytes,at+44);
                if (bin==0 || bin<=lastBin || flags>1 || (flags==1)!=(id!=0) || firstVertex!=vertexEnd || firstIndex!=indexEnd ||
                    vertexCount==0 || indexCount==0 || indexCount%3!=0 || (ulong)firstVertex+vertexCount>vertices || (ulong)firstIndex+indexCount>indices ||
                    !Finite(sourceHeight)||sourceHeight<=0||!Finite(east)||!Finite(north)) throw new ArgumentException("Invalid slice building row.");
                if (id!=0)
                {
                    if (featuredCount==4) throw new ArgumentException("Too many featured buildings.");
                    for (int j=0;j<featuredCount;j++) if(featured[j]==id) throw new ArgumentException("Duplicate featured identity.");
                    featured[featuredCount++]=id;
                }
                for(uint j=firstIndex;j<firstIndex+indexCount;j++)
                {
                    uint index=U32(bytes,indexAt+(int)j*4);
                    if(index<firstVertex || index>=(ulong)firstVertex+vertexCount) throw new ArgumentException("Invalid slice mesh index.");
                }
                rows[i]=new SliceBuilding { Bin=bin,Id=id,FirstVertex=(int)firstVertex,VertexCount=(int)vertexCount,FirstIndex=(int)firstIndex,IndexCount=(int)indexCount,Height=sourceHeight,Centroid=new Vector2(east,north) };
                vertexEnd+=vertexCount;indexEnd+=indexCount;lastBin=bin;
            }
            if(featuredCount!=4 || vertexEnd!=vertices || indexEnd!=indices) throw new ArgumentException("Incomplete slice geometry partition.");
            Array.Sort(featured);
            var positions=new Vector3[(int)vertices];var normals=new Vector3[(int)vertices];var triangles=new int[(int)indices];
            for(int i=0;i<positions.Length;i++)
            {
                Vector3 p=UnityVector(bytes,vertexAt+i*24),n=UnityVector(bytes,vertexAt+i*24+12);
                if(!Finite(p)||!Finite(n)||Math.Abs(n.sqrMagnitude-1)>0.0001f ||
                    p.x<minimum.x||p.x>maximum.x||p.y<minimum.y||p.y>maximum.y||p.z<minimum.z||p.z>maximum.z) throw new ArgumentException("Invalid slice vertex/normal.");
                positions[i]=p;normals[i]=n;
            }
            // Reflecting ENU→Unity reverses handedness; reverse each triangle exactly once.
            for(int i=0;i<triangles.Length;i+=3)
            {
                int a=(int)U32(bytes,indexAt+i*4),b=(int)U32(bytes,indexAt+(i+2)*4),c=(int)U32(bytes,indexAt+(i+1)*4);
                if(Vector3.Dot(Vector3.Cross(positions[b]-positions[a],positions[c]-positions[a]),normals[a])<=0) throw new ArgumentException("Degenerate or inward slice triangle.");
                triangles[i]=a;triangles[i+1]=b;triangles[i+2]=c;
            }
            var bounds=new Bounds();bounds.SetMinMax(minimum,maximum);
            return new SliceGeometry(rows,positions,normals,triangles,featured,bounds,latitude,longitude,height);
        }

        static SliceResident Resident(NativeArray<byte> b,int at) => new SliceResident {
            Id=U64(b,at),Governing=U64(b,at+8),Foot=UnityVector(b,at+16),Heading=F32(b,at+28),Detail=U32(b,at+32),Activity=U32(b,at+36),Location=U32(b,at+40)
        };
        static SliceBuildingSummary Summary(NativeArray<byte> b,int at) => new SliceBuildingSummary {
            Id=U64(b,at),Mode=U32(b,at+8),Occupancy=U32(b,at+12),Governed=U32(b,at+16),Sleep=U32(b,at+20),Commute=U32(b,at+24),Work=U32(b,at+28),School=U32(b,at+32),Errands=U32(b,at+36),Home=U32(b,at+40)
        };
        /// <summary>Validate the entire boundary before publishing into reusable arrays; no success-path allocation.</summary>
        public static ulong DecodeSnapshot(NativeArray<byte> bytes,int length,SliceGeometry geometry,SliceFrame frame)
        {
            RequireStorage(bytes,length,SnapshotHeader);
            if(geometry==null||frame==null) throw new ArgumentException("Missing slice destination/geometry.");
            uint population=U32(bytes,48),count=U32(bytes,52),buildings=U32(bytes,56),scale=U32(bytes,44);
            if(U32(bytes,4)!=128 || U64(bytes,32)!=SliceGeometry.DatasetVersion || U32(bytes,40)!=1 || !ValidScale(scale) ||
                population==0||population>30||count!=population||buildings!=4||U32(bytes,60)!=48||U32(bytes,64)!=48||U32(bytes,68)!=0 ||
                128UL+(count+buildings)*48UL!=(ulong)length) throw new ArgumentException("Invalid slice snapshot layout/count.");
            for(int at=92;at<128;at+=4) if(U32(bytes,at)!=0) throw new ArgumentException("Invalid slice reserved header.");
            uint stationary=U32(bytes,72),transit=U32(bytes,76),offmap=U32(bytes,80),l1=U32(bytes,84),l2=U32(bytes,88);
            if((ulong)stationary+transit+offmap!=population || (ulong)l1+l2!=population) throw new ArgumentException("Invalid slice count partition.");
            ulong previous=0;uint actualL1=0;int rendered=0;
            for(int i=0;i<count;i++)
            {
                int at=128+i*48;SliceResident r=Resident(bytes,at);
                if(r.Id==0||r.Id<=previous||geometry.IsFeatured(r.Id)||!geometry.IsFeatured(r.Governing)||!Finite(r.Foot)||!Finite(r.Heading)||Math.Abs(r.Heading)>Mathf.PI ||
                    (r.Detail!=1&&r.Detail!=2)||r.Activity>5||r.Location>3||(r.Detail==2)!=(r.Location==0)||U32(bytes,at+44)!=0)
                    throw new ArgumentException("Invalid slice resident row.");
                if(r.Location!=1 && (U32(bytes,at+16)!=0||U32(bytes,at+20)!=0||U32(bytes,at+24)!=0||U32(bytes,at+28)!=0)) throw new ArgumentException("Nonlocal resident carries geometry.");
                if(r.Foot.sqrMagnitude>1000000000000f) throw new ArgumentException("Excessive slice position.");
                if(r.Detail==1) actualL1++;if(r.Location==1) rendered++;previous=r.Id;
            }
            if(actualL1!=l1) throw new ArgumentException("Invalid slice level count.");
            int summaryAt=128+(int)count*48;ulong occupancyTotal=0,governedTotal=0;
            for(int i=0;i<4;i++)
            {
                int at=summaryAt+i*48;var s=Summary(bytes,at);
                ulong activities=(ulong)s.Sleep+s.Commute+s.Work+s.School+s.Errands+s.Home;
                if(s.Id!=geometry.FeaturedIds[i]||s.Mode<1||s.Mode>3||s.Occupancy>population||s.Governed>population||activities!=s.Occupancy||U32(bytes,at+44)!=0)
                    throw new ArgumentException("Invalid slice building summary.");
                uint governed=0;
                for(int j=0;j<count;j++) if(U64(bytes,128+j*48+8)==s.Id) governed++;
                if(governed!=s.Governed) throw new ArgumentException("Invalid slice governing partition.");
                occupancyTotal+=s.Occupancy;governedTotal+=s.Governed;
            }
            if(occupancyTotal!=stationary||governedTotal!=population) throw new ArgumentException("Invalid slice summary totals.");
            int visible=0;
            for(int i=0;i<count;i++)
            {
                var r=Resident(bytes,128+i*48);frame.Residents[i]=r;
                if(r.Location==1)
                {
                    var forward=new Vector3(Mathf.Cos(r.Heading),0,Mathf.Sin(r.Heading));
                    frame.ResidentMatrices[visible]=Matrix4x4.TRS(r.Foot+Vector3.up*0.85f,Quaternion.LookRotation(forward,Vector3.up),new Vector3(0.5f,0.85f,0.5f));
                    frame.RenderedResidentIds[visible++]=r.Id;
                }
            }
            for(int i=0;i<4;i++) frame.Buildings[i]=Summary(bytes,summaryAt+i*48);
            frame.Population=(int)population;frame.RenderedResidents=rendered;frame.Tick=U64(bytes,8);frame.SimTimeMicroseconds=unchecked((long)U64(bytes,16));frame.StateHash=U64(bytes,24);
            frame.Scale=scale;frame.LocalStationary=stationary;frame.InTransit=transit;frame.OffMapStationary=offmap;frame.L1=l1;frame.L2=l2;
            return frame.Tick;
        }
    }
}
