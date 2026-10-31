using System;
using Unity.Collections;

namespace WorldSim.Bridge
{
    /// <summary>One daily goal row (ADR0039 §3). Kind 1..4; progress never exceeds target unless done.</summary>
    public struct SliceGoal
    {
        public uint Kind,Target,Progress;
        public bool Done;
    }
    /// <summary>Immutable-on-publication faith and goals sample (ADR0039 §5). Rows ascend by resident ID.</summary>
    public sealed class SliceGod
    {
        public long Day;
        public int BlockFaith;
        public SliceGoal[] Goals=Array.Empty<SliceGoal>();
        public (ulong Id,int Faith)[] Faith=Array.Empty<(ulong,int)>();
        /// <summary>Faith of one resident, zero when the ID is not a retained resident.</summary>
        public int FaithOf(ulong id)
        {
            int lo=0,hi=Faith.Length-1;
            while(lo<=hi)
            {
                int mid=(lo+hi)>>1;ulong at=Faith[mid].Id;
                if(at==id) return Faith[mid].Faith;
                if(at<id) lo=mid+1;else hi=mid-1;
            }
            return 0;
        }
    }
    /// <summary>Decode-then-validate for the 560-byte WsSliceGod; a bad record is never published.</summary>
    public static class SliceGodDecoder
    {
        public const int Size=560;
        static void Require(bool valid,string why) {if(!valid) throw new InvalidOperationException("Invalid native god record: "+why);}
        public static SliceGod Decode(NativeArray<byte> b)
        {
            Require(b.Length==Size,"length");
            Require(SliceDecoder.U32(b,0)==Size,"struct_size");Require(SliceDecoder.U32(b,4)==0,"reserved");
            var god=new SliceGod {Day=unchecked((long)SliceDecoder.U64(b,8)),BlockFaith=unchecked((int)SliceDecoder.U32(b,16))};
            uint goalCount=SliceDecoder.U32(b,20);Require(goalCount<=3,"goal_count");
            god.Goals=new SliceGoal[goalCount];
            for(int i=0;i<3;i++)
            {
                int at=24+i*16;uint kind=SliceDecoder.U32(b,at),target=SliceDecoder.U32(b,at+4),progress=SliceDecoder.U32(b,at+8),done=SliceDecoder.U32(b,at+12);
                if(i>=goalCount) {Require(kind==0&&target==0&&progress==0&&done==0,"unused goal row");continue;}
                Require(kind>=1&&kind<=4,"goal kind");Require(done<=1,"done flag");Require(done==1||progress<=target,"progress");Require(done==0||progress>=target,"done progress");
                god.Goals[i]=new SliceGoal {Kind=kind,Target=target,Progress=progress,Done=done==1};
            }
            uint count=SliceDecoder.U32(b,72);Require(count<=30,"resident_count");Require(SliceDecoder.U32(b,76)==0,"reserved2");
            god.Faith=new (ulong,int)[count];
            long sum=0;
            for(int i=0;i<30;i++)
            {
                int at=80+i*16;ulong id=SliceDecoder.U64(b,at);int faith=unchecked((int)SliceDecoder.U32(b,at+8));uint reserved=SliceDecoder.U32(b,at+12);
                if(i>=count) {Require(id==0&&faith==0&&reserved==0,"unused resident row");continue;}
                Require(reserved==0,"row reserved");Require(id!=0&&(i==0||id>god.Faith[i-1].Id),"resident order");Require(faith>=-100&&faith<=100,"faith range");
                god.Faith[i]=(id,faith);sum+=faith;
            }
            Require(god.BlockFaith==(count==0?0:(int)(sum/count)),"block_faith mean"); // C# division truncates toward zero.
            return god;
        }
    }
}
