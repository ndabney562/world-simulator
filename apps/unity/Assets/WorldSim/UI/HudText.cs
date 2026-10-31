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
        /// <summary>One feed line for an illness-consequence event (tags 5, 6, 10-22); null for any other tag.</summary>
        public static string Feed(InspectionEvent e,Func<ulong,string> name)
        {
            switch(e.Tag)
            {
                case 13:return e.E==0?name(e.A)+" is already unwell; the curse had no effect.":"You cursed "+name(e.A)+" with illness.";
                case 5:return name(e.A)+" fell sick.";
                case 10:return name(e.A)+" is sick and missed "+(e.C==2?"work.":"school.");
                case 11:return name(e.A)+" checked on their neighbour "+name(e.B)+".";
                case 6:return name(e.A)+" went home to rest.";
                case 12:return name(e.A)+" recovered.";
                case 14:return name(e.A)+" was blessed";
                case 15:return name(e.A)+" feels refreshed";
                case 16:return "A storm rolls in";
                case 17:return name(e.A)+" stayed inside";
                case 18:return "The storm kept "+e.A.ToString(CultureInfo.InvariantCulture)+" people inside";
                case 19:return "The storm has passed";
                case 20:return "A gathering was called";
                case 21:return name(e.A)+" joined the gathering";
                case 22:return "The gathering ended ("+e.B.ToString(CultureInfo.InvariantCulture)+" came)";
                default:return null;
            }
        }
        /// <summary>True once a consequence chain can produce no further events.</summary>
        public static bool Finished(SliceConsequences c)
        {
            // Bless (14) commits whole; a storm is done at 19; a gathering once 22 is seen and every
            // joined attendee has gone home (6 rows). Illness keeps its recovery / no-effect rule.
            ulong attendees=0;int returned=0;bool ended=false;
            foreach(var e in c.Events)
            {
                if(e.Tag==12||e.Tag==14||e.Tag==19||(e.Tag==13&&e.E==0)) return true;
                if(e.Tag==22) {ended=true;attendees=e.B;} else if(e.Tag==6) returned++;
            }
            return ended&&(ulong)returned>=attendees;
        }
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
