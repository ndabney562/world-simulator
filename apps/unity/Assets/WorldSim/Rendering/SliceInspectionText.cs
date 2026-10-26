using System.Globalization;
using System.Text;
using WorldSim.Bridge;

namespace WorldSim.Rendering
{
    public static class SliceInspectionText
    {
        static readonly string[] Activities={"Sleep","Commute","Work","School","Errands","Home"};
        static readonly string[] Employment={"Not employed","Student","Employed","Retired"};
        public static string Consequences(SliceIllnessSample sample)
        {
            var text=new System.Text.StringBuilder("Illness consequences\nHealth: ");text.Append(sample.Inspection.Health).Append("/100\n");
            for(int i=0;i<sample.Consequences.Events.Length;i++) {
                var e=sample.Consequences.Events[i];string name=e.Tag==13?(e.E==0?"Already affected (no change)":"Curse applied"):e.Tag==5?"Illness began":e.Tag==6?"Returned home":e.Tag==10?(e.C==2?"Missed work":"Missed school"):e.Tag==11?"Neighbour check-in":e.Tag==12?"Recovered":"Unknown";
                text.Append(name);
                foreach(var cause in sample.Consequences.Causes[i]) {
                    text.Append(cause.Confidence==0?(cause.Relation==3?" · Direct contribution":" · Direct"):" · Contributing");
                    string parent="Illness";foreach(var prior in sample.Consequences.Events) if(prior.Id==cause.Parent&&prior.Tag==13) parent="Curse";
                    text.Append(" ← ").Append(parent);
                }
                text.Append('\n');
            }
            return text.ToString();
        }
        public static string Format(SliceInspection r)
        {
            if(r.Kind==1)
            {
                return string.Format(CultureInfo.InvariantCulture,
                    "Age: {0}\nSex: {1}\nEmployment: {2}\nHealth: {3}/100\nAlive/dead: {4}",
                    r.Age,r.Sex==0?"Female":"Male",Employment[r.Role],r.Health,r.Life==0?"Alive":"Dead");
            }
            var text=new StringBuilder(4096);var sky=r.Sky;
            text.AppendFormat(CultureInfo.InvariantCulture,"Snapshot UTC {0:D4}-{1:D2}-{2:D2} {3:D2}:{4:D2}:{5:D2}\nTick {6} · tap again to refresh\n",sky.UtcYear,sky.UtcMonth,sky.UtcDay,sky.UtcHour,sky.UtcMinute,sky.UtcSecond,r.Tick);
            text.Append(r.Address).Append('\n').Append(r.BuildingRole).Append(" · class ").Append(r.BuildingClass).Append('\n');
            text.AppendFormat(CultureInfo.InvariantCulture,"BIN {0} · BBL {1}\nAbove-ground height {2:F2}m · {3} residential units\n",r.Bin,r.Bbl,r.HeightMm/1000.0,r.Units);
            text.Append("Mode L").Append(r.Summary.Mode).Append(" · current occupants ").Append(r.Summary.Occupancy).Append(" · governed people ").Append(r.Summary.Governed).Append('\n');
            for(uint i=0;i<6;i++) text.Append(Activities[i]).Append(' ').Append(r.Summary.Activity(i)).Append(i==5?'\n':' ');
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
