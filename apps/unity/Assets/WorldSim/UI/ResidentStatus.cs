using System.Collections.Generic;
using WorldSim.Bridge;

namespace WorldSim.UI
{
    /// <summary>The one icon shown above a resident; higher values win when several apply.</summary>
    public enum StatusIcon {None,Recovering,Visiting,Socialising,Sheltering,Sick}

    /// <summary>
    /// Pure per-resident status built from the consequence rows the HUD already polls. Each status
    /// holds an end time in sim microseconds (long.MaxValue while open) and, where the row names it,
    /// the building the resident is in. Nothing here queries the core or writes simulation state.
    /// </summary>
    public sealed class ResidentStatus
    {
        public const long RecoveringUs=3600000000;
        const int Kinds=6;
        sealed class Entry {public readonly long[] Until=new long[Kinds];public readonly ulong[] Building=new ulong[Kinds];}
        readonly SortedDictionary<ulong,Entry> entries=new SortedDictionary<ulong,Entry>();
        /// <summary>Sim time the active storm ends, or 0 when none is under way.</summary>
        public long StormUntil {get;private set;}
        /// <summary>Residents with any recorded status, in ascending ID order.</summary>
        public IEnumerable<ulong> Residents=>entries.Keys;

        Entry Of(ulong id)
        {
            if(!entries.TryGetValue(id,out var e)) {e=new Entry();entries[id]=e;}
            return e;
        }
        void Set(ulong id,StatusIcon icon,long until,ulong building=0)
        {
            var e=Of(id);e.Until[(int)icon]=until;e.Building[(int)icon]=building;
        }
        static long Open(long end)=>end>0?end:long.MaxValue;

        /// <summary>A storm the player just cast, before its first rows are polled.</summary>
        public void BeginStorm(long until) {StormUntil=until;}

        /// <summary>Applies one consequence row once. <paramref name="rootTag"/> and <paramref name="end"/>
        /// are the row's group root tag and scheduled end (0 when the group has none).</summary>
        public void Apply(InspectionEvent e,ushort rootTag,long end)
        {
            switch(e.Tag)
            {
                case 13:if(e.E!=0) Set(e.A,StatusIcon.Sick,Open(end));break;
                case 5:Set(e.A,StatusIcon.Sick,Open(end));break;
                case 12:Set(e.A,StatusIcon.Sick,0);Set(e.A,StatusIcon.Recovering,e.Time+RecoveringUs);break;
                case 11:Set(e.A,StatusIcon.Visiting,e.Time+(long)e.D,e.C);break;
                case 16:StormUntil=Open(end);break;
                case 17:Set(e.A,StatusIcon.Sheltering,Open(end),e.B);break;
                case 19:
                    StormUntil=0;
                    foreach(var entry in entries.Values) entry.Until[(int)StatusIcon.Sheltering]=0;
                    break;
                case 21:Set(e.A,StatusIcon.Socialising,end>0?end+HudText.GatheringGraceUs:long.MaxValue,e.B);break;
                case 6:if(rootTag==20) Set(e.A,StatusIcon.Socialising,0);break;
            }
        }
        /// <summary>The highest-priority status active at <paramref name="now"/>, and the building it names (0 if none).</summary>
        public StatusIcon Get(ulong id,long now,out ulong building)
        {
            building=0;
            if(!entries.TryGetValue(id,out var e)) return StatusIcon.None;
            for(int k=Kinds-1;k>0;k--)
                if(e.Until[k]>now) {building=e.Building[k];return (StatusIcon)k;}
            return StatusIcon.None;
        }
        public StatusIcon Get(ulong id,long now)=>Get(id,now,out _);
        public bool StormActive(long now)=>StormUntil>now;
        /// <summary>A load replaces the world: every earlier row belongs to another history.</summary>
        public void Clear() {entries.Clear();StormUntil=0;}
    }
}
