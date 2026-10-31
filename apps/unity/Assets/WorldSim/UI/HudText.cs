using System;
using System.Collections.Generic;
using System.Globalization;
using WorldSim.Bridge;

namespace WorldSim.UI
{
    /// <summary>Plain-language HUD text. Pure: names come from the caller, never from a query here.</summary>
    public static class HudText
    {
        static readonly string[] Activities={"Sleeping","Commuting","Working","At school","Running errands","At home"};
        static readonly string[] Jobs={"Not employed","Student","Employed","Retired"};
        public static string Activity(uint activity)=>activity<Activities.Length?Activities[activity]:"Unknown";
        public static string Job(uint role)=>role<Jobs.Length?Jobs[role]:"Unknown";
        public static string Name(SliceInspection r)=>r.GivenName+" "+r.FamilyName;
        /// <summary>One feed line for a consequence event (tags 5, 6, 10-22); null for any other tag.
        /// <paramref name="rootTag"/> is the group's root tag: a ReturnedHome (6) in a gathering group (20)
        /// reads as heading home, not as going home to rest.</summary>
        public static string Feed(InspectionEvent e,Func<ulong,string> name,ushort rootTag=13)
        {
            switch(e.Tag)
            {
                case 13:return e.E==0?name(e.A)+" is already unwell; the curse had no effect.":"You cursed "+name(e.A)+" with illness.";
                case 5:return name(e.A)+" fell sick.";
                case 10:return name(e.A)+" is sick and missed "+(e.C==2?"work.":"school.");
                case 11:return name(e.A)+" checked on their neighbour "+name(e.B)+".";
                case 6:return rootTag==20?name(e.A)+" headed home":name(e.A)+" went home to rest.";
                case 12:return name(e.A)+" recovered.";
                case 14:return name(e.A)+" was blessed";
                case 15:return name(e.A)+" feels refreshed";
                case 16:return "A storm rolls in";
                case 17:return name(e.A)+" stayed inside";
                case 18:return e.A==1?"The storm kept 1 person inside":"The storm kept "+e.A.ToString(CultureInfo.InvariantCulture)+" people inside";
                case 19:return "The storm has passed";
                case 20:return "A gathering was called";
                case 21:return name(e.A)+" joined the gathering";
                case 22:return "The gathering ended ("+e.B.ToString(CultureInfo.InvariantCulture)+" came)";
                default:return null;
            }
        }
        /// <summary>Gathering watches stop at the latest 30 minutes after the gathering's end, so an
        /// attendee whose ReturnedHome went to another root (an illness interruption) cannot hold it open.</summary>
        public const long GatheringGraceUs=1800000000;
        /// <summary>True once a consequence chain can produce no further events. <c>c.Time</c> is the
        /// sim time of the read, paired with the sky by the bridge.</summary>
        public static bool Finished(SliceConsequences c)
        {
            // Bless (14) commits whole; a storm is done at 19. Illness ends at its recovery (12), a
            // no-effect curse, or its scheduled end: a bless cure keeps that end and never adds a 12
            // here, and nothing in the group can follow it. A gathering is done once 22 is seen and
            // either every joiner (21) has its own ReturnedHome (6) in the group, or the grace cap has
            // passed. Non-joiners' 6 rows never count, so a late walker cannot finish it early.
            var joined=new HashSet<ulong>();var home=new HashSet<ulong>();bool ended=false;
            foreach(var e in c.Events)
            {
                if(e.Tag==12||e.Tag==14||e.Tag==19||(e.Tag==13&&e.E==0)) return true;
                if(e.Tag==22) ended=true;else if(e.Tag==21) joined.Add(e.A);else if(e.Tag==6) home.Add(e.A);
            }
            if(c.RootTag==20) return ended&&(home.IsSupersetOf(joined)||(decimal)c.Time>=(decimal)c.End+GatheringGraceUs);
            return (c.RootTag==13||c.RootTag==0)&&c.End!=0&&c.Time>=c.End;
        }
        /// <summary>Faith meter text: "Faith +12", "Faith 0", "Faith -7" (ASCII minus).</summary>
        public static string Faith(int blockFaith)=>blockFaith==0?"Faith 0":"Faith "+(blockFaith>0?"+":"")+blockFaith.ToString(CultureInfo.InvariantCulture);
        /// <summary>The goal in plain words, singular for a target of 1 (kinds 1-4: gathering, bless, cure, storm).</summary>
        public static string Goal(SliceGoal g)
        {
            string n=g.Target.ToString(CultureInfo.InvariantCulture);bool one=g.Target==1;
            switch(g.Kind)
            {
                case 1:return one?"Bring 1 person to a gathering":"Bring "+n+" people to a gathering";
                case 2:return one?"Bless 1 person":"Bless "+n+" different people";
                case 3:return "Cure someone with a blessing";
                case 4:return one?"Keep 1 person inside during a storm":"Keep "+n+" people inside during a storm";
                default:return "Goal";
            }
        }
        /// <summary>Goal-list row: "{goal} \u2014 {p}/{N}", or "Done: {goal}".</summary>
        public static string GoalLine(SliceGoal g)=>g.Done?"Done: "+Goal(g):Goal(g)+" \u2014 "+g.Progress.ToString(CultureInfo.InvariantCulture)+"/"+g.Target.ToString(CultureInfo.InvariantCulture);
        public static string GoalComplete(SliceGoal g)=>"Goal complete: "+Goal(g);
        /// <summary>How a resident feels about the player: devoted (&gt;= 40), trusting (&gt;= 10), unsure (&gt; -10), wary (&gt; -40), fearful.</summary>
        public static string FaithWord(int faith)=>faith>=40?"devoted":faith>=10?"trusting":faith>-10?"unsure":faith>-40?"wary":"fearful";
        public static string CardFaith(int faith)=>"Faith: "+FaithWord(faith);
        public static string Resident(SliceInspection r)
        {
            return string.Format(CultureInfo.InvariantCulture,"Age {0} · {1}\nHealth {2}/100{3}\nNow: {4}",
                r.Age,Job(r.Role),r.Health,r.Life==1?" · Dead":r.IllnessActive?" · Sick":"",Activity(r.Activity));
        }
        public static string Building(SliceInspection r)
        {
            return string.Format(CultureInfo.InvariantCulture,"{0}\n{1} · {2} homes\n{3} people here now",r.Address,r.BuildingRole,r.Units,r.Summary.Occupancy);
        }
        /// <summary>Today's rows ("HH:MM Activity"), using the decoder's model-week rule: days 0-4 weekday table, 5-6 weekend.</summary>
        public static List<string> TodaySchedule(SliceInspection r)
        {
            var lines=new List<string>();
            decimal delta=(decimal)r.Time-r.ScheduleEpoch,dayUs=86400000000m;
            long day=(long)decimal.Floor(delta/dayUs);uint table=(day%7+7)%7<5?0u:1u;
            foreach(var row in r.Schedule)
                if(row.Table==table) lines.Add(string.Format(CultureInfo.InvariantCulture,"{0:D2}:{1:D2} {2}",row.Minute/60,row.Minute%60,Activity(row.Activity)));
            return lines;
        }
    }
}
