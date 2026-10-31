using System;
using System.Text;
using Unity.Collections;

namespace WorldSim.Bridge
{
    public struct InspectionSchedule
    {
        public uint Table,Minute,Activity,DestinationKind,OriginKind,Arrival,DestinationExit,OriginExit;
        public ulong Destination,Origin;
    }
    public struct InspectionEvent
    {
        public ulong Id,Tick,Ordinal,A,B,C,D,E;
        public long Time;
        public uint Tag;
    }
    /// <summary>One immutable-on-publication tap sample. Never refreshed by the render loop.</summary>
    public sealed class SliceInspection
    {
        public ulong Id,Tick,StateHash,Dataset,Home,Household,Workplace,Bin,Bbl,From,To;
        public long Time,ScheduleEpoch,IllnessEnd;
        public uint OverrideKind;public bool IllnessActive;public ulong OverrideRoot,OwnRoot;
        public uint Kind,Scale,Age,Role,Sex,Health,Life,Detail,Activity,Action,Location,FromExit,ToExit,HeightMm,Units;
        public int Jitter;
        public bool MoreEvents;
        public string GivenName="",FamilyName="",Address="",BuildingClass="",BuildingRole="";
        public readonly uint[] Deficits=new uint[4];
        public SliceBuildingSummary Summary;
        public InspectionSchedule[] Schedule=Array.Empty<InspectionSchedule>();
        public InspectionEvent[] Events=Array.Empty<InspectionEvent>();
        public CalendarSky Sky;
    }
    public static class InspectionDecoder
    {
        static readonly UTF8Encoding Utf8=new UTF8Encoding(false,true);
        static uint U32(NativeArray<byte> b,int at)=>SliceDecoder.U32(b,at);
        static ulong U64(NativeArray<byte> b,int at)=>SliceDecoder.U64(b,at);
        static void Require(bool valid) {if(!valid) throw new ArgumentException("Invalid native inspection record.");}
        static bool Scale(uint n)=>n==0||n==1||n==10||n==100;
        static string Text(NativeArray<byte> b,ref int at,int end)
        {
            Require(at<=end-4);uint count=U32(b,at);at+=4;Require(count<=128&&count<=end-at);
            var bytes=new byte[(int)count];for(int i=0;i<bytes.Length;i++) {bytes[i]=b[at++];Require(bytes[i]!=0);}
            return Utf8.GetString(bytes);
        }
        static bool Grid(ulong us,ulong min,ulong max)=>us>=min&&us<=max&&us%100000==0;
        static bool Endpoint(SliceGeometry g,ulong id,uint exit)=>id!=0&&(exit==uint.MaxValue?g.IsFeatured(id):exit<=1&&!g.IsFeatured(id));
        internal static void ValidateIllnessEvent(InspectionEvent e,uint kind,ulong id,SliceGeometry geometry)
        {
            if(e.Tag==5) Require(kind==1&&e.A==id&&e.B==0&&e.C>=1&&e.C<=10&&e.D==0&&e.E==0);
            else if(e.Tag==6) Require(e.A!=0&&geometry.IsFeatured(e.B)&&e.C==0&&e.D==0&&e.E==0&&(kind==1?e.A==id:e.B==id));
            else if(e.Tag==10) Require(e.A!=0&&e.B!=0&&(e.C==2||e.C==3)&&e.D==0&&e.E==0&&(kind==1?e.A==id:e.B==id));
            else if(e.Tag==11) Require(e.A!=0&&e.B!=0&&e.A!=e.B&&geometry.IsFeatured(e.C)&&e.D==300000000&&e.E==0&&(kind==1?e.A==id||e.B==id:e.C==id));
            else if(e.Tag==12) Require(kind==1&&e.A==id&&e.B==100&&e.C==0&&e.D==0&&e.E==0);
            else if(e.Tag==13) Require(kind==1&&e.A==id&&e.C>=1&&e.C<=10&&e.D>=100000&&e.D<=86400000000&&e.D%100000==0&&e.E<=1);
            // ADR0038 §5 power rows: five words each, as pinned by the Rust event_words layout test.
            else if(e.Tag==14) Require(kind==1&&e.A==id&&e.C<=1&&e.D==0&&e.E==0);
            else if(e.Tag==15) Require(kind==1&&e.A==id&&e.B==0&&e.C==0&&e.D==0&&e.E==0);
            else if(e.Tag==16) Require(Grid(e.B,100000,3600000000)&&e.C==0&&e.D==0&&e.E==0);
            else if(e.Tag==17) Require(e.A!=0&&!geometry.IsFeatured(e.A)&&geometry.IsFeatured(e.B)&&e.C<=5&&e.D==0&&e.E==0&&(kind==1?e.A==id:e.B==id));
            else if(e.Tag==18||e.Tag==19) Require(e.B==0&&e.C==0&&e.D==0&&e.E==0);
            else if(e.Tag==20) Require(kind==2&&e.A==id&&Grid(e.C,600000000,3600000000)&&e.D<=7&&e.E==0);
            else if(e.Tag==21) Require(e.A!=0&&!geometry.IsFeatured(e.A)&&geometry.IsFeatured(e.B)&&e.C==0&&e.D==0&&e.E==0&&(kind==1?e.A==id:e.B==id));
            else if(e.Tag==22) Require(kind==2&&e.A==id&&e.B<=7&&e.C==0&&e.D==0&&e.E==0);
            else throw new ArgumentException("Unsupported inspection event vocabulary.");
        }
        /// <summary>Validate a whole bounded record before returning a new published value.</summary>
        public static SliceInspection Decode(NativeArray<byte> b,int length,SliceGeometry geometry,ulong expectedId)
        {
            Require(b.IsCreated&&length>=96&&length<=8192&&length<=b.Length&&geometry!=null&&expectedId!=0);
            NativeWorld.RequireVersion(U32(b,0));Require(U32(b,4)==96&&U32(b,48)==3&&U32(b,56)==length);
            uint kind=U32(b,52),identity=U32(b,64),state=U32(b,68),rows=U32(b,72),events=U32(b,76);
            Require((kind==1||kind==2)&&U64(b,40)==expectedId&&U64(b,32)==SliceGeometry.DatasetVersion&&
                U32(b,60)<=1&&identity>=8&&identity<=1024&&state==(kind==1?112:48)&&rows<=32&&events<=16&&
                U32(b,80)==48&&U32(b,84)==80&&Scale(U32(b,88))&&U32(b,92)==0&&
                96UL+identity+state+rows*48UL+events*80UL==(ulong)length&&
                (kind==1?!geometry.IsFeatured(expectedId):geometry.IsFeatured(expectedId)));
            var r=new SliceInspection {Id=expectedId,Tick=U64(b,8),Time=unchecked((long)U64(b,16)),StateHash=U64(b,24),Dataset=U64(b,32),
                Kind=kind,Scale=U32(b,88),MoreEvents=U32(b,60)==1};
            int at=96,end=96+(int)identity;Require(U32(b,at)==2&&U32(b,at+4)==identity);at+=8;
            if(kind==1)
            {
                Require(at<=end-36);r.Home=U64(b,at);r.Household=U64(b,at+8);r.Workplace=U64(b,at+16);r.Age=U32(b,at+24);r.Role=U32(b,at+28);r.Sex=U32(b,at+32);at+=36;
                r.GivenName=Text(b,ref at,end);r.FamilyName=Text(b,ref at,end);
                Require(geometry.IsFeatured(r.Home)&&r.Household!=0&&r.Age<=90&&r.Role<=3&&r.Sex<=1&&r.GivenName.Length>0&&r.FamilyName.Length>0&&
                    ((r.Role==1||r.Role==2)?r.Workplace!=0:r.Workplace==0));
            }
            else
            {
                Require(at<=end-24);r.Bin=U64(b,at);r.Bbl=U64(b,at+8);r.HeightMm=U32(b,at+16);r.Units=U32(b,at+20);at+=24;
                r.Address=Text(b,ref at,end);r.BuildingClass=Text(b,ref at,end);r.BuildingRole=Text(b,ref at,end);
                Require(r.Bin!=0&&r.Bbl!=0&&r.HeightMm>0&&r.Address.Length>0&&r.BuildingClass.Length>0&&r.BuildingRole.Length>0);
                bool found=false;foreach(var building in geometry.Buildings) if(building.Id==r.Id&&building.Bin==r.Bin) found=true;Require(found);
            }
            Require(at==end);int stateAt=end;
            if(kind==1)
            {
                r.Detail=U32(b,stateAt);r.Activity=U32(b,stateAt+4);r.Action=U32(b,stateAt+8);r.Location=U32(b,stateAt+12);
                for(int i=0;i<4;i++) {r.Deficits[i]=U32(b,stateAt+16+i*4);Require(r.Deficits[i]<=100000);}
                r.From=U64(b,stateAt+32);r.To=U64(b,stateAt+40);r.FromExit=U32(b,stateAt+48);r.ToExit=U32(b,stateAt+52);
                r.Health=U32(b,stateAt+56);r.Life=U32(b,stateAt+60);
                r.OverrideKind=U32(b,stateAt+80);r.IllnessActive=U32(b,stateAt+84)==1;r.OverrideRoot=U64(b,stateAt+88);r.OwnRoot=U64(b,stateAt+96);r.IllnessEnd=unchecked((long)U64(b,stateAt+104));
                // Kinds: 0 none, 1 illness hold, 2 visit, 3 gathering. Only kind3 may stand away from
                // home, with the Home activity and Socialize at a featured building (ADR0038 §4).
                Require(r.OverrideKind<=3&&U32(b,stateAt+84)<=1&&(r.OverrideKind==0)==(r.OverrideRoot==0)&&
                    (r.IllnessActive?r.Life==0&&r.Health>=1&&r.Health<100&&r.OwnRoot!=0&&r.IllnessEnd>r.Time&&r.OverrideKind==1&&r.OverrideRoot==r.OwnRoot:r.IllnessEnd==0)&&
                    (r.OverrideKind==0||r.Activity==1||r.Activity==5&&r.Location==0&&(r.From==r.Home||r.OverrideKind==3&&r.Action==3))&&
                    (r.OverrideKind!=1||r.Activity==1&&r.Action==5||r.Activity==5&&r.Action==1)&&
                    (r.OverrideKind!=2||r.Activity==5&&r.Action==3&&r.Health==100&&r.Life==0)&&
                    (r.OverrideKind!=3||r.Activity==1&&r.Action==5||r.Activity==5&&(r.Action==3||r.Action==1)));
                r.ScheduleEpoch=unchecked((long)U64(b,stateAt+64));r.Jitter=unchecked((int)U32(b,stateAt+72));
                Require(r.Detail>=1&&r.Detail<=2&&r.Activity<=5&&r.Action<=6&&r.Location<=2&&r.Health<=100&&r.Life<=1&&(r.Life==0?r.Health>=1:r.Health==0)&&
                    r.Jitter>=-15&&r.Jitter<=15&&U32(b,stateAt+76)==1&&Endpoint(geometry,r.From,r.FromExit)&&Endpoint(geometry,r.To,r.ToExit)&&
                    (r.Location==2)==(r.Activity==1)&&(r.Location==2||r.From==r.To&&r.FromExit==r.ToExit)&&
                    (r.Location!=0||r.FromExit==uint.MaxValue)&&(r.Location!=1||r.FromExit<=1));
            }
            else
            {
                var summary=new SliceBuildingSummary {Id=U64(b,stateAt),Mode=U32(b,stateAt+8),Occupancy=U32(b,stateAt+12),Governed=U32(b,stateAt+16),
                    Sleep=U32(b,stateAt+20),Commute=U32(b,stateAt+24),Work=U32(b,stateAt+28),School=U32(b,stateAt+32),Errands=U32(b,stateAt+36),Home=U32(b,stateAt+40)};
                ulong total=(ulong)summary.Sleep+summary.Commute+summary.Work+summary.School+summary.Errands+summary.Home;
                Require(rows==0&&summary.Id==expectedId&&summary.Mode>=1&&summary.Mode<=3&&summary.Occupancy<=30&&summary.Governed<=30&&total==summary.Occupancy&&U32(b,stateAt+44)==0);r.Summary=summary;
            }
            at=stateAt+(int)state;r.Schedule=new InspectionSchedule[(int)rows];uint previousTable=0,previousMinute=0;bool sawWeekend=false;
            for(int i=0;i<r.Schedule.Length;i++,at+=48)
            {
                var row=new InspectionSchedule {Table=U32(b,at),Minute=U32(b,at+4),Activity=U32(b,at+8),DestinationKind=U32(b,at+12),OriginKind=U32(b,at+16),
                    Arrival=U32(b,at+20),DestinationExit=U32(b,at+24),OriginExit=U32(b,at+28),Destination=U64(b,at+32),Origin=U64(b,at+40)};
                Require(row.Table<=1&&row.Minute<1440&&row.Activity<=5&&row.DestinationKind<=1&&row.OriginKind<=2&&
                    Endpoint(geometry,row.Destination,row.DestinationExit)&&row.DestinationKind==(row.DestinationExit==uint.MaxValue?0u:1u));
                bool first=i==0||row.Table!=previousTable;Require(row.Table>=previousTable&&(first?row.Minute==0:row.Minute>previousMinute));
                if(row.Table==1) sawWeekend=true;
                if(row.OriginKind==0) Require(row.Origin==0&&row.OriginExit==uint.MaxValue&&row.Arrival==uint.MaxValue&&row.Activity!=1);
                else Require(row.Activity==1&&row.Arrival>row.Minute&&row.Arrival<=1440&&Endpoint(geometry,row.Origin,row.OriginExit)&&row.OriginKind==(row.OriginExit==uint.MaxValue?1u:2u));
                r.Schedule[i]=row;previousTable=row.Table;previousMinute=row.Minute;
            }
            if(kind==1)
            {
                Require(rows>0&&r.Schedule[0].Table==0&&sawWeekend);
                for(int i=0;i<r.Schedule.Length;i++)
                {
                    var row=r.Schedule[i];if(row.OriginKind==0) continue;
                    Require(i>0&&i+1<r.Schedule.Length);
                    var previous=r.Schedule[i-1];var next=r.Schedule[i+1];
                    Require(previous.Table==row.Table&&next.Table==row.Table&&previous.OriginKind==0&&next.OriginKind==0&&
                        previous.Destination==row.Origin&&previous.DestinationExit==row.OriginExit&&row.Arrival==next.Minute&&
                        next.Destination==row.Destination&&next.DestinationExit==row.DestinationExit);
                }
                // Decimal is exact for the difference of two i64 microsecond instants.
                // This preserves Euclidean model-week arithmetic without signed overflow.
                decimal delta=(decimal)r.Time-r.ScheduleEpoch,dayUs=86400000000m;
                long day=(long)decimal.Floor(delta/dayUs);uint table=(day%7+7)%7<5?0u:1u;
                decimal within=delta%dayUs;if(within<0) within+=dayUs;uint minute=(uint)(within/60000000m);
                InspectionSchedule current=default;
                foreach(var row in r.Schedule) if(row.Table==table&&row.Minute<=minute) current=row;
                if(r.OverrideKind==0) {
                Require(r.Activity==current.Activity);
                if(current.OriginKind==0) Require(r.Location==(current.DestinationKind==0?0u:1u)&&r.To==current.Destination&&r.ToExit==current.DestinationExit);
                else Require(r.Location==2&&r.From==current.Origin&&r.FromExit==current.OriginExit&&r.To==current.Destination&&r.ToExit==current.DestinationExit);
                } else if(r.Location==2) {
                    bool baselineLeg=false,knownFrom=false;
                    foreach(var row in r.Schedule) {
                        if(row.Destination==r.From&&row.DestinationExit==r.FromExit||row.OriginKind!=0&&row.Origin==r.From&&row.OriginExit==r.FromExit) knownFrom=true;
                        if(row.OriginKind!=0&&row.Origin==r.From&&row.OriginExit==r.FromExit&&row.Destination==r.To&&row.DestinationExit==r.ToExit) baselineLeg=true;
                    }
                    // Kind3 legs run home to the featured target and back on the navigation graph.
                    Require(r.Action==5&&(r.OverrideKind==1&&(r.To==r.Home&&r.ToExit==uint.MaxValue&&knownFrom||baselineLeg)||
                        r.OverrideKind==3&&r.FromExit==uint.MaxValue&&r.ToExit==uint.MaxValue&&(r.From==r.Home||r.To==r.Home)));
                }
            }
            r.Events=new InspectionEvent[(int)events];
            for(int i=0;i<r.Events.Length;i++,at+=80)
            {
                var e=new InspectionEvent {Id=U64(b,at),Time=unchecked((long)U64(b,at+8)),Tick=U64(b,at+16),Ordinal=U64(b,at+24),Tag=U32(b,at+32),
                    A=U64(b,at+40),B=U64(b,at+48),C=U64(b,at+56),D=U64(b,at+64),E=U64(b,at+72)};
                Require(e.Id!=0&&e.Ordinal!=0&&e.Tick<=r.Tick&&e.Time<=r.Time&&U32(b,at+36)==1);
                if(i>0) Require(e.Time<r.Events[i-1].Time||(e.Time==r.Events[i-1].Time&&e.Id<r.Events[i-1].Id));
                for(int j=0;j<i;j++) Require(r.Events[j].Id!=e.Id);
                if(e.Tag==7) Require(e.A!=0&&e.B!=0&&e.C<=5&&e.D<=5&&e.C!=e.D&&e.E==0&&(kind==1?e.A==r.Id:e.B==r.Id));
                else if(e.Tag==9) Require(kind==2&&e.A==r.Id&&e.B>=1&&e.B<=3&&e.C>=1&&e.C<=3&&e.D<=30&&e.E<=30&&(e.B!=e.C||e.D!=0||e.E!=0));
                else ValidateIllnessEvent(e,kind,r.Id,geometry);
                r.Events[i]=e;
            }
            Require(at==length&&(!r.MoreEvents||events==16));return r;
        }
    }
    public struct ConsequenceCause {public ulong Parent;public uint Relation,Confidence;}
    public sealed class SliceConsequences
    {
        public ulong Tick,StateHash,Dataset,Root,Subject;public long Time,Applied,End;public uint Severity;
        /// <summary>Payload tag of the root row: 13 illness, 14 bless, 16 storm, 20 gathering.</summary>
        public ushort RootTag;
        public InspectionEvent[] Events=Array.Empty<InspectionEvent>();
        public ConsequenceCause[][] Causes=Array.Empty<ConsequenceCause[]>();
    }
    public sealed class SliceIllnessSample {public SliceInspection Inspection;public SliceConsequences Consequences;}
    public static class ConsequencesDecoder
    {
        static void Require(bool valid) {if(!valid) throw new ArgumentException("Invalid native consequences record.");}
        /// <summary>ADR0038 §5: header and row rules are per root tag; illness (13) keeps ADR0034's rules exactly.</summary>
        public static SliceConsequences Decode(NativeArray<byte> b,int length,SliceGeometry geometry,ulong expectedRoot)
        {
            Require(b.IsCreated&&length>=96&&length<=8192&&length<=b.Length&&geometry!=null&&expectedRoot!=0);
            NativeWorld.RequireVersion(SliceDecoder.U32(b,0));
            uint count=SliceDecoder.U32(b,60);
            Require(SliceDecoder.U32(b,4)==96&&SliceDecoder.U32(b,56)==count&&count>=1&&count<=16&&SliceDecoder.U32(b,64)==128&&
                SliceDecoder.U32(b,68)==0&&SliceDecoder.U32(b,72)==1&&length==96+count*128&&SliceDecoder.U64(b,40)==expectedRoot&&
                SliceDecoder.U64(b,32)==SliceGeometry.DatasetVersion);
            uint root=SliceDecoder.U32(b,96+32);
            if(root!=13&&root!=14&&root!=16&&root!=20) throw new ArgumentException("Unsupported consequence root.");
            var r=new SliceConsequences {Tick=SliceDecoder.U64(b,8),Time=unchecked((long)SliceDecoder.U64(b,16)),StateHash=SliceDecoder.U64(b,24),Dataset=SliceDecoder.U64(b,32),
                Root=expectedRoot,RootTag=(ushort)root,Subject=SliceDecoder.U64(b,48),Severity=SliceDecoder.U32(b,76),Applied=unchecked((long)SliceDecoder.U64(b,80)),End=unchecked((long)SliceDecoder.U64(b,88)),
                Events=new InspectionEvent[count],Causes=new ConsequenceCause[count][]};
            Require(r.Applied<=r.Time&&(root==13?r.Subject!=0&&!geometry.IsFeatured(r.Subject)&&r.Severity>=1&&r.Severity<=10:
                r.Severity==0&&(root==14?r.Subject!=0&&!geometry.IsFeatured(r.Subject)&&r.End==r.Applied:root==16?r.Subject==0:geometry.IsFeatured(r.Subject))));
            for(int i=0,at=96;i<count;i++,at+=128)
            {
                var e=new InspectionEvent {Id=SliceDecoder.U64(b,at),Time=unchecked((long)SliceDecoder.U64(b,at+8)),Tick=SliceDecoder.U64(b,at+16),Ordinal=SliceDecoder.U64(b,at+24),Tag=SliceDecoder.U32(b,at+32),
                    A=SliceDecoder.U64(b,at+40),B=SliceDecoder.U64(b,at+48),C=SliceDecoder.U64(b,at+56),D=SliceDecoder.U64(b,at+64),E=SliceDecoder.U64(b,at+72)};
                Require(e.Id!=0&&e.Ordinal!=0&&e.Time>=r.Applied&&e.Time<=r.Time&&e.Tick<=r.Tick&&SliceDecoder.U32(b,at+36)==1&&
                    (i==0?e.Id==r.Root:e.Id>r.Events[i-1].Id)&&SliceDecoder.U32(b,at+84)==0&&SliceDecoder.U64(b,at+120)==0);
                uint causes=SliceDecoder.U32(b,at+80);Require(causes<=2);r.Causes[i]=new ConsequenceCause[causes];
                for(int j=0;j<2;j++)
                {
                    int offset=at+88+j*16;var c=new ConsequenceCause {Parent=SliceDecoder.U64(b,offset),Relation=SliceDecoder.U32(b,offset+8),Confidence=SliceDecoder.U32(b,offset+12)};
                    if(j>=causes) Require(c.Parent==0&&c.Relation==0&&c.Confidence==0);else r.Causes[i][j]=c;
                }
                if(root==13) IllnessRow(r,i,e,causes,count,geometry);else PowerRow(root,r,i,e,count,geometry);
                r.Events[i]=e;
            }
            return r;
        }
        /// <summary>ADR0034 illness group rows, unchanged.</summary>
        static void IllnessRow(SliceConsequences r,int i,InspectionEvent e,uint causes,uint count,SliceGeometry geometry)
        {
            InspectionDecoder.ValidateIllnessEvent(e,1,r.Subject,geometry);
            if(i==0) Require(e.Tag==13&&e.Time==r.Applied&&e.C==r.Severity&&causes==0&&
                (e.E==0?r.End==0&&count==1:count>=2&&(decimal)r.End-r.Applied==e.D));
            else Require(e.Tag!=13&&causes==1);
            if(e.Tag==5) Require(i==1&&e.Time==r.Applied&&e.C==r.Severity);
            if(e.Tag==11) Require(e.B==r.Subject&&e.Time<r.End);
            if(e.Tag==12) Require(e.Time==r.End);
            int priorMisses=0;for(int k=0;k<i;k++) {if(r.Events[k].Tag==10) priorMisses++;Require(e.Tag==10||e.Tag!=r.Events[k].Tag);}
            if(e.Tag==10) Require(priorMisses<2);
            foreach(var c in r.Causes[i])
            {
                int parent=-1;for(int k=0;k<i;k++) if(r.Events[k].Id==c.Parent) parent=k;
                Require(parent>=0&&r.Events[parent].Time<=e.Time&&c.Relation<=3&&c.Confidence<=3);
                Require(e.Tag==5?parent==0&&c.Relation==0&&c.Confidence==0:r.Events[parent].Tag==5&&
                    (e.Tag==11?c.Relation==3&&c.Confidence==1:e.Tag==6?c.Relation==3&&c.Confidence==0:c.Relation==0&&c.Confidence==0));
            }
        }
        /// <summary>ADR0038 §5 bless (14), storm (16) and gathering (20) groups. Every effect is
        /// CausedBy/Direct the root; a bless cure also contributes from the earlier illness event.</summary>
        static void PowerRow(uint root,SliceConsequences r,int i,InspectionEvent e,uint count,SliceGeometry geometry)
        {
            var causes=r.Causes[i];
            if(i==0)
            {
                Require(e.Tag==root&&e.Time==r.Applied&&causes.Length==0);
                if(root==14) {InspectionDecoder.ValidateIllnessEvent(e,1,r.Subject,geometry);Require(count==1+e.C);}
                else if(root==16) {InspectionDecoder.ValidateIllnessEvent(e,0,0,geometry);Require((decimal)r.End-r.Applied==e.B&&(count==1||count==3));}
                else {InspectionDecoder.ValidateIllnessEvent(e,2,r.Subject,geometry);Require((decimal)r.End-r.Applied==e.C);}
                return;
            }
            Require(causes.Length>=1&&causes[0].Parent==r.Root&&causes[0].Relation==0&&causes[0].Confidence==0);
            var first=r.Events[0];
            if(root==14)
            {
                InspectionDecoder.ValidateIllnessEvent(e,1,r.Subject,geometry);
                Require(i==1&&e.Time==r.Applied&&(e.Tag==15&&causes.Length==1||e.Tag==12&&causes.Length==2&&
                    causes[1].Parent!=0&&causes[1].Parent<r.Root&&causes[1].Relation==3&&causes[1].Confidence==0));
            }
            else if(root==16)
            {
                InspectionDecoder.ValidateIllnessEvent(e,0,0,geometry);
                Require(causes.Length==1&&e.Tag==(i==1?18u:19u)&&e.Time==r.End&&(e.Tag!=19||e.A==first.A));
            }
            else
            {
                // Attendee rows repeat: up to the called count of joins and returns, one end.
                Require(causes.Length==1);
                int joined=0,returned=0;bool ended=false;
                for(int k=1;k<i;k++)
                {
                    var prior=r.Events[k];
                    if(prior.Tag==21) {joined++;Require(e.Tag!=21||prior.A!=e.A);}
                    else if(prior.Tag==6) {returned++;Require(e.Tag!=6||prior.A!=e.A);}
                    else ended=true;
                }
                if(e.Tag==21) {InspectionDecoder.ValidateIllnessEvent(e,2,r.Subject,geometry);Require(!ended&&e.Time<r.End&&(ulong)joined<first.D);}
                else if(e.Tag==6) {InspectionDecoder.ValidateIllnessEvent(e,1,e.A,geometry);Require(e.Time>=r.End&&(ulong)returned<first.D);}
                else if(e.Tag==22) {InspectionDecoder.ValidateIllnessEvent(e,2,r.Subject,geometry);Require(!ended&&e.Time==r.End&&e.B==(ulong)joined);}
                else Require(false);
            }
        }
    }

}
