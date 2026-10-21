using System;
using System.Globalization;
using System.Text;
using WorldSim.Bridge;

namespace WorldSim.Rendering
{
    public static class SliceInspectionText
    {
        static readonly string[] Activities={"Sleep","Commute","Work","School","Errands","Home"};
        static readonly string[] Actions={"Idle","Rest","Eat","Socialize","Duties","Travel","Sleep"};
        static readonly string[] Roles={"Preschool","Student","Employed","Retired"};
        static string Minute(uint minute)=>string.Format(CultureInfo.InvariantCulture,"{0:D2}:{1:D2}",minute/60,minute%60);
        static string Place(ulong id,uint exit)=>exit==uint.MaxValue?"building "+id:"off-map workplace "+id+" (exit "+exit+")";
        public static string Format(SliceInspection r)
        {
            var text=new StringBuilder(4096);var sky=r.Sky;
            text.AppendFormat(CultureInfo.InvariantCulture,"Snapshot UTC {0:D4}-{1:D2}-{2:D2} {3:D2}:{4:D2}:{5:D2}\nTick {6} · tap again to refresh\n",sky.UtcYear,sky.UtcMonth,sky.UtcDay,sky.UtcHour,sky.UtcMinute,sky.UtcSecond,r.Tick);
            if(r.Kind==1)
            {
                text.Append(r.GivenName).Append(' ').Append(r.FamilyName).Append(" · synthetic resident\n").Append("Age ").Append(r.Age).Append(" · ").Append(Roles[r.Role]).Append(" · L").Append(r.Detail).Append('\n');
                text.Append("Home ").Append(r.Home).Append(" · household ").Append(r.Household).Append('\n');
                if(r.Workplace!=0) text.Append("Assigned workplace ").Append(r.Workplace).Append('\n');
                text.Append("Activity: ").Append(Activities[r.Activity]).Append(" · held action: ").Append(Actions[r.Action]).Append('\n');
                text.Append(r.Location==2?"In transit: ":"At: ").Append(Place(r.From,r.FromExit));
                if(r.Location==2) text.Append(" → ").Append(Place(r.To,r.ToExit));text.Append('\n');
                text.AppendFormat(CultureInfo.InvariantCulture,"Need deficits /100000 (lower is better)\nRest {0} · Hunger {1} · Social {2} · Duty {3}\n",r.Deficits[0],r.Deficits[1],r.Deficits[2],r.Deficits[3]);
                text.Append("Model timetable (elapsed week; shifted ").Append(r.Jitter).Append(" minutes)\n");
                uint previous=uint.MaxValue;foreach(var row in r.Schedule)
                {
                    if(row.Table!=previous) {text.Append(row.Table==0?"Weekdays:\n":"Weekend:\n");previous=row.Table;}
                    text.Append(Minute(row.Minute)).Append(' ').Append(Activities[row.Activity]).Append(" → ").Append(Place(row.Destination,row.DestinationExit)).Append('\n');
                }
            }
            else
            {
                text.Append(r.Address).Append('\n').Append(r.BuildingRole).Append(" · class ").Append(r.BuildingClass).Append('\n');
                text.AppendFormat(CultureInfo.InvariantCulture,"BIN {0} · BBL {1}\nAbove-ground height {2:F2}m · {3} residential units\n",r.Bin,r.Bbl,r.HeightMm/1000.0,r.Units);
                text.Append("Mode L").Append(r.Summary.Mode).Append(" · current occupants ").Append(r.Summary.Occupancy).Append(" · governed people ").Append(r.Summary.Governed).Append('\n');
                for(uint i=0;i<6;i++) text.Append(Activities[i]).Append(' ').Append(r.Summary.Activity(i)).Append(i==5?'\n':' ');
            }
            text.Append("Recent recorded actor events:\n");
            if(r.Events.Length==0) text.Append("No matching recorded events.\n");
            foreach(var e in r.Events)
            {
                double seconds=((double)r.Time-e.Time)/1000000;
                text.AppendFormat(CultureInfo.InvariantCulture,"#{0} · {1:F1}s ago · ",e.Id,seconds);
                if(e.Tag==7) text.Append("Schedule ").Append(Activities[e.C]).Append(" → ").Append(Activities[e.D]).Append("; resident ").Append(e.A).Append("; destination ").Append(e.B);
                else text.Append("Building detail L").Append(e.B).Append(" → L").Append(e.C).Append("; demoted ").Append(e.D).Append("; promoted ").Append(e.E);
                text.Append('\n');
            }
            if(r.MoreEvents) text.Append("Showing latest 16; more recorded events exist.\n");
            return text.ToString();
        }
    }
}
