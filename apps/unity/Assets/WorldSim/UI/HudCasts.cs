using System;
using System.Collections.Generic;

namespace WorldSim.UI
{
    /// <summary>One cast the HUD polls: its root, the root's payload tag and, for resident powers, the subject.</summary>
    public sealed class HudWatch {public ulong Subject,Root;public ushort RootTag;public bool Done;}
    /// <summary>
    /// Pure cast decisions for the game HUD: which root tag a power's watch carries, when a
    /// block-wide power is refused before the bridge is called, and the player-facing warning for
    /// a rejection. Raw bridge text never reaches the player.
    /// </summary>
    public static class HudCasts
    {
        public const ushort CurseTag=13,BlessTag=14,StormTag=16,GatherTag=20;
        public const string Busy="A storm or gathering is already under way.";
        public const string Rejected="That didn't work. Try another target.";
        /// <summary>A storm or a gathering this HUD cast and has not seen finish.</summary>
        public static bool BlockPowerUnderWay(IReadOnlyList<HudWatch> watches)
        {
            foreach(var w in watches) if(!w.Done&&(w.RootTag==StormTag||w.RootTag==GatherTag)) return true;
            return false;
        }
        /// <summary>Casts once through <paramref name="cast"/> and adds its watch. Returns null on success,
        /// otherwise the warning to show; a refused or rejected cast adds no watch.</summary>
        public static string Try(List<HudWatch> watches,ushort tag,ulong subject,Func<ulong> cast)
        {
            if((tag==StormTag||tag==GatherTag)&&BlockPowerUnderWay(watches)) return Busy;
            ulong root;
            try {root=cast();}
            catch(Exception) {return Rejected;}
            Watch(watches,tag,subject,root);
            return null;
        }
        /// <summary>Adds the watch for an already-applied root (the curse applies through SliceDemo).</summary>
        public static void Watch(List<HudWatch> watches,ushort tag,ulong subject,ulong root)
        {
            watches.Add(new HudWatch {Subject=tag==StormTag?0:subject,Root=root,RootTag=tag});
        }
    }
}
