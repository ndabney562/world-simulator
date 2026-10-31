using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UIElements;
using WorldSim.Bridge;
using WorldSim.Rendering;

namespace WorldSim.UI
{
    /// <summary>
    /// UI Toolkit game HUD: time controls, resident card, event feed and power bar.
    /// Presentation only: every action goes through SliceDemo's existing calls.
    /// </summary>
    [RequireComponent(typeof(SliceDemo))]
    public sealed class GameHud : MonoBehaviour
    {
        enum Power {None,Curse,Bless,Gather}
        public const ulong StormUs=3600000000,GatherUs=1800000000;
        const int FeedLines=8;
        const double PollSeconds=1;
        const int GoalRows=GoalWatch.MaxGoals;
        static readonly Color Panel=new Color(0.07f,0.08f,0.11f,0.92f),Accent=new Color(0.95f,0.7f,0.25f),Text=new Color(0.92f,0.92f,0.9f),Dim=new Color(0.62f,0.64f,0.68f);
        SliceDemo demo;
        UIDocument document;
        PanelSettings settings;
        ThemeStyleSheet theme;
        VisualElement root,card,controls,feedPanel,powerBar,schedule;
        Label calendar,notice,stats,cardTitle,cardBody,cardFaith,faithMeter,feedEmpty,hint;
        readonly Label[] goalLabels=new Label[GoalRows];
        Button curse,bless,storm,gather,closeCard,follow;
        Reactions reactions;
        readonly Dictionary<uint,Button> speeds=new Dictionary<uint,Button>();
        readonly List<HudWatch> watches=new List<HudWatch>();
        readonly HashSet<ulong> seen=new HashSet<ulong>();
        readonly Dictionary<ulong,string> names=new Dictionary<ulong,string>();
        readonly List<string> feed=new List<string>();
        readonly List<Label> feedLabels=new List<Label>();
        SliceInspection shown;
        Power armed;bool built;
        double nextPoll,warningUntil;
        string warning="";
        // Faith and goals: polled at most once a real second; strings are built on the poll only.
        SliceGod god;double nextGod;bool godWarned;
        readonly GoalWatch goalWatch=new GoalWatch();
        readonly List<SliceGoal> completedGoals=new List<SliceGoal>();
        public string MeterText=>faithMeter!=null?faithMeter.text:"";
        public string CardFaithText=>cardFaith!=null&&cardFaith.resolvedStyle.display!=DisplayStyle.None?cardFaith.text:"";
        public bool PowerArmed=>armed!=Power.None;
        public IReadOnlyList<string> Feed=>feed;
        /// <summary>Each consequence row the first time a poll sees it, with its group.</summary>
        public event System.Action<InspectionEvent,SliceConsequences> ConsequenceSeen;
        /// <summary>A power that applied: its root tag and subject (0 for a storm).</summary>
        public event System.Action<ushort,ulong> Casted;
        /// <summary>Select or deselect the power; the next resident tap casts it.</summary>
        public void TogglePower() {Toggle(Power.Curse);}
        /// <summary>Select or deselect bless; the next resident tap casts it.</summary>
        public void ToggleBless() {Toggle(Power.Bless);}
        /// <summary>Select or deselect gathering; the next featured-building tap casts it.</summary>
        public void ToggleGather() {Toggle(Power.Gather);}
        void Toggle(Power p) {armed=armed==p?Power.None:p;}
        /// <summary>Storm needs no target: it casts at once for one hour.</summary>
        public void CastStorm()
        {
            // A failed cast leaves any armed power armed.
            if(Cast(HudCasts.StormTag,0,()=>demo.Storm(StormUs))) armed=Power.None;
        }
        /// <summary>Watches the HUD is polling, for tests.</summary>
        public IReadOnlyList<HudWatch> Watches=>watches;
        /// <summary>The warning line currently shown, or empty, for tests.</summary>
        public string Warning=>Time.realtimeSinceStartupAsDouble<warningUntil?warning:"";
        bool Cast(ushort tag,ulong subject,System.Func<ulong> cast)
        {
            string refused=HudCasts.Try(watches,tag,subject,cast);
            if(refused!=null) {Warn(refused);return false;}
            nextPoll=0;Casted?.Invoke(tag,subject);return true;
        }

        void OnEnable()
        {
            demo=GetComponent<SliceDemo>();reactions=GetComponent<Reactions>();
            settings=ScriptableObject.CreateInstance<PanelSettings>();theme=ScriptableObject.CreateInstance<ThemeStyleSheet>();
            settings.name="Game HUD panel";settings.themeStyleSheet=theme;settings.scaleMode=PanelScaleMode.ConstantPixelSize;settings.scale=SliceDemo.HudUnits;
            document=gameObject.AddComponent<UIDocument>();document.panelSettings=settings;
            demo.UiHitTest=OverHud;demo.Inspected+=OnInspected;demo.Cast+=OnCast;demo.WorldReplaced+=OnWorldReplaced;
        }
        void OnDisable()
        {
            if(demo!=null) {if(demo.UiHitTest==OverHud) demo.UiHitTest=null;demo.Inspected-=OnInspected;demo.Cast-=OnCast;demo.WorldReplaced-=OnWorldReplaced;}
            if(document!=null) Destroy(document);document=null;root=null;built=false;
            if(settings!=null) Destroy(settings);if(theme!=null) Destroy(theme);settings=null;theme=null;
            watches.Clear();seen.Clear();names.Clear();god=null;goalWatch.Reset();nextGod=0;feed.Clear();feedLabels.Clear();speeds.Clear();shown=null;armed=Power.None;
        }
        bool OverHud(Vector2 screen)
        {
            var panel=root?.panel;if(panel==null) return false;
            var picked=panel.Pick(RuntimePanelUtils.ScreenToPanel(panel,new Vector2(screen.x,Screen.height-screen.y)));
            return picked!=null&&picked!=root;
        }
        static VisualElement Box(VisualElement parent,Color background)
        {
            var e=new VisualElement();var s=e.style;s.backgroundColor=background;s.paddingLeft=s.paddingRight=10;s.paddingTop=s.paddingBottom=8;
            s.borderTopLeftRadius=s.borderTopRightRadius=s.borderBottomLeftRadius=s.borderBottomRightRadius=8;parent.Add(e);return e;
        }
        static Label Line(VisualElement parent,string text,Color colour,int size=13)
        {
            var l=new Label(text);l.style.color=colour;l.style.fontSize=size;l.style.whiteSpace=WhiteSpace.Normal;l.pickingMode=PickingMode.Ignore;parent.Add(l);return l;
        }
        static Button Action(VisualElement parent,string text,System.Action click)
        {
            var b=new Button(click) {text=text};var s=b.style;
            s.backgroundColor=new Color(0.2f,0.22f,0.28f);s.color=Text;s.fontSize=13;s.marginRight=6;s.marginTop=4;s.paddingLeft=s.paddingRight=10;s.paddingTop=s.paddingBottom=5;
            s.borderTopLeftRadius=s.borderTopRightRadius=s.borderBottomLeftRadius=s.borderBottomRightRadius=6;
            s.borderLeftWidth=s.borderRightWidth=s.borderTopWidth=s.borderBottomWidth=1;s.borderLeftColor=s.borderRightColor=s.borderTopColor=s.borderBottomColor=new Color(1,1,1,0.12f);
            parent.Add(b);return b;
        }
        static VisualElement Row(VisualElement parent)
        {
            var r=new VisualElement();r.style.flexDirection=FlexDirection.Row;r.style.flexWrap=Wrap.Wrap;r.pickingMode=PickingMode.Ignore;parent.Add(r);return r;
        }
        void Build()
        {
            root=document.rootVisualElement;if(root==null) return;
            root.Clear();root.pickingMode=PickingMode.Ignore;root.style.flexGrow=1;
            // The runtime theme is empty, so text needs an explicit (built-in, always shipped) font.
            root.style.unityFontDefinition=FontDefinition.FromFont(Resources.GetBuiltinResource<Font>("LegacyRuntime.ttf"));
            // Top strip: the camera viewport excludes it, so it never hides the block.
            var strip=new VisualElement();var s=strip.style;s.position=Position.Absolute;s.left=s.right=s.top=0;s.height=SliceDemo.HudStripHeight;
            s.backgroundColor=Color.black;s.flexDirection=FlexDirection.Row;s.paddingLeft=s.paddingRight=s.paddingTop=s.paddingBottom=8;root.Add(strip);

            controls=Box(strip,Panel);controls.style.width=300;controls.style.flexShrink=0;controls.style.marginRight=8;
            calendar=Line(controls,"",Text,14);
            var time=Row(controls);
            foreach(var (scale,label) in new[]{(0u,"Pause"),(1u,"x1"),(10u,"x10"),(100u,"x100")}) {uint value=scale;speeds[value]=Action(time,label,()=>demo.SetSpeed(value));}
            var view=Row(controls);Action(view,"Overview",demo.ResetView);Action(view,"Street",demo.RequestStreetView);Action(view,"Save",demo.SaveSlice);Action(view,"Load",demo.LoadSlice);
            notice=Line(controls,"",Accent,12);
            stats=Line(controls,"",Dim,10);
            faithMeter=Line(controls,"Faith 0",Accent,13);faithMeter.style.marginTop=4;
            for(int i=0;i<GoalRows;i++) {goalLabels[i]=Line(controls,"",Text,11);goalLabels[i].style.display=DisplayStyle.None;}

            card=Box(strip,Panel);card.style.width=320;card.style.flexShrink=1;card.style.marginRight=8;
            var head=Row(card);head.style.justifyContent=Justify.SpaceBetween;head.style.flexWrap=Wrap.NoWrap;
            cardTitle=Line(head,"",Accent,16);
            var buttons=Row(head);buttons.style.flexWrap=Wrap.NoWrap;
            follow=Action(buttons,"Follow",ToggleFollow);follow.style.marginTop=0;
            closeCard=Action(buttons,"Close",demo.ClearInspection);closeCard.style.marginTop=0;
            cardBody=Line(card,"",Text,13);
            cardFaith=Line(card,"",Dim,13);
            var scroll=new ScrollView(ScrollViewMode.Vertical);scroll.style.flexGrow=1;scroll.style.marginTop=4;card.Add(scroll);schedule=scroll.contentContainer;

            feedPanel=Box(strip,Panel);feedPanel.style.flexGrow=1;feedPanel.style.flexShrink=1;feedPanel.style.minWidth=200;
            Line(feedPanel,"What's happening",Accent,14);
            feedEmpty=Line(feedPanel,"Choose a power below, then tap a target.",Dim,12);
            for(int i=0;i<FeedLines;i++) {var l=Line(feedPanel,"",Text,12);l.style.marginTop=2;feedLabels.Add(l);}

            // Power bar: one button per god power; select, then tap a resident.
            powerBar=new VisualElement();var p=powerBar.style;p.position=Position.Absolute;p.left=p.right=0;p.bottom=12;p.alignItems=Align.Center;powerBar.pickingMode=PickingMode.Ignore;root.Add(powerBar);
            hint=Line(powerBar,"",Accent,13);
            var bar=Box(powerBar,Panel);bar.style.flexDirection=FlexDirection.Row;
            curse=Action(bar,"Curse with illness",TogglePower);curse.style.fontSize=15;
            bless=Action(bar,"Bless",ToggleBless);bless.style.fontSize=15;
            storm=Action(bar,"Storm",CastStorm);storm.style.fontSize=15;
            gather=Action(bar,"Gathering",ToggleGather);gather.style.fontSize=15;
            built=true;
        }
        void Warn(string text) {warning=text;warningUntil=Time.realtimeSinceStartupAsDouble+3;}
        void OnInspected(SliceInspection r)
        {
            if(armed==Power.None) return;
            if(armed==Power.Gather)
            {
                var geometry=demo.Geometry;
                if(r.Kind!=2||geometry==null||!geometry.IsFeatured(r.Id)) {Warn("Gatherings need a home: tap a building.");return;}
                if(Cast(HudCasts.GatherTag,r.Id,()=>demo.Gather(r.Id,GatherUs))) armed=Power.None;
                return;
            }
            if(r.Kind!=1) {Warn(armed==Power.Bless?"Bless needs a person: tap a resident.":"Curses need a person: tap a resident.");return;}
            if(armed==Power.Bless)
            {
                if(Cast(HudCasts.BlessTag,r.Id,()=>demo.Bless(r.Id))) armed=Power.None;
                return;
            }
            // Fresh tap sample has no root, so this applies the curse exactly once.
            if(demo.InspectIllness()) armed=Power.None;else Warn(HudCasts.Rejected);
        }
        void OnCast(ulong subject,ulong root) {HudCasts.Watch(watches,HudCasts.CurseTag,subject,root);nextPoll=0;Casted?.Invoke(HudCasts.CurseTag,subject);}
        /// <summary>Follow the resident on the card, or stop following.</summary>
        public void ToggleFollow()
        {
            var r=demo.CurrentInspection;if(reactions==null||r==null||r.Kind!=1) return;
            reactions.Follow(reactions.Following==r.Id?0:r.Id);
        }
        void OnWorldReplaced()
        {
            watches.Clear();seen.Clear();names.Clear();goalWatch.Reset();god=null;nextGod=0;Add("Loaded a saved world.");
        }
        string NameOf(ulong id)
        {
            if(!names.TryGetValue(id,out var name)) {name=demo.ResidentName(id);names[id]=name;}
            return name;
        }
        void Add(string line)
        {
            feed.Insert(0,line);if(feed.Count>FeedLines) feed.RemoveAt(feed.Count-1);
        }
        void Poll()
        {
            foreach(var w in watches)
            {
                if(w.Done) continue;
                SliceConsequences c;
                try {c=demo.Consequences(w.Subject,w.Root,w.RootTag);} catch(System.Exception) {w.Done=true;continue;}
                foreach(var e in c.Events)
                {
                    if(!seen.Add(e.Id)) continue;
                    ConsequenceSeen?.Invoke(e,c);
                    string line=HudText.Feed(e,NameOf,c.RootTag);if(line!=null) Add(line);
                }
                w.Done=HudText.Finished(c);
            }
            watches.RemoveAll(w=>w.Done);
        }
        /// <summary>Reads faith and goals (at most once a second), refreshes the meter, goal rows and card line, and
        /// adds a feed line when a goal turns done between two polls of the same day.</summary>
        void PollGod(double now)
        {
            SliceGod g;
            try {g=demo.God();} catch(System.Exception e) {nextGod=now+PollSeconds;if(!godWarned) {godWarned=true;Debug.LogWarning("Faith poll failed: "+e.Message);}return;}
            nextGod=now+PollSeconds;
            int n=System.Math.Min(g.Goals.Length,GoalRows);
            completedGoals.Clear();goalWatch.Update(g,completedGoals);
            foreach(var c in completedGoals) Add(HudText.GoalComplete(c));
            god=g;
            faithMeter.text=HudText.Faith(g.BlockFaith);
            for(int i=0;i<GoalRows;i++)
            {
                bool on=i<n;goalLabels[i].style.display=on?DisplayStyle.Flex:DisplayStyle.None;
                if(on) goalLabels[i].text=HudText.GoalLine(g.Goals[i]);
            }
            UpdateCardFaith();
        }
        void UpdateCardFaith()
        {
            var r=shown;
            bool on=r!=null&&r.Kind==1&&god!=null;
            cardFaith.style.display=on?DisplayStyle.Flex:DisplayStyle.None;
            if(on) cardFaith.text=HudText.CardFaith(god.FaithOf(r.Id));
        }
        void Update()
        {
            if(document==null||demo==null) return;
            if(settings.scale!=SliceDemo.HudUnits) settings.scale=SliceDemo.HudUnits;
            if(!built) {Build();if(!built) return;}
            double now=Time.realtimeSinceStartupAsDouble;
            // A new cast polls once at any speed (its events are already committed); repeats
            // wait while paused, since chains cannot progress and each poll forces a pacing boundary.
            if(watches.Count>0&&now>=nextPoll&&(nextPoll==0||demo.CurrentFrame.Scale!=0)) {Poll();nextPoll=now+PollSeconds;}
            if(now>=nextGod) PollGod(now);
            Refresh();
        }
        void Refresh()
        {
            calendar.text=demo.CalendarText;notice.text=demo.Notice;stats.text=demo.StatusText;
            uint current=demo.CurrentFrame.Scale;
            foreach(var pair in speeds) pair.Value.style.backgroundColor=pair.Key==current?new Color(0.55f,0.4f,0.12f):new Color(0.2f,0.22f,0.28f);
            foreach(var (button,power) in new[]{(curse,Power.Curse),(bless,Power.Bless),(gather,Power.Gather)})
                button.style.backgroundColor=armed==power?new Color(0.55f,0.12f,0.2f):new Color(0.2f,0.22f,0.28f);
            hint.text=Time.realtimeSinceStartupAsDouble<warningUntil?warning:armed==Power.Gather?"Tap a building to cast.":armed!=Power.None?"Tap a resident to cast.":"";
            var inspection=demo.CurrentInspection;
            if(inspection!=shown)
            {
                shown=inspection;schedule.Clear();
                if(inspection!=null)
                {
                    cardTitle.text=inspection.Kind==1?HudText.Name(inspection):"Building";
                    cardBody.text=inspection.Kind==1?HudText.Resident(inspection):HudText.Building(inspection);
                    if(inspection.Kind==1) {Line(schedule,"Today",Accent,12);foreach(var row in HudText.TodaySchedule(inspection)) Line(schedule,row,Dim,12);}
                }
                UpdateCardFaith();
            }
            card.style.display=inspection!=null?DisplayStyle.Flex:DisplayStyle.None;
            bool resident=inspection!=null&&inspection.Kind==1&&reactions!=null;
            follow.style.display=resident?DisplayStyle.Flex:DisplayStyle.None;
            if(resident) follow.text=reactions.Following==inspection.Id?"Stop following":"Follow";
            // Narrow screens: the card takes the feed's place while it is open.
            bool narrow=Screen.width/SliceDemo.HudUnits<900;
            feedPanel.style.display=narrow&&inspection!=null?DisplayStyle.None:DisplayStyle.Flex;
            controls.style.width=narrow?220:300;
            feedEmpty.style.display=feed.Count==0?DisplayStyle.Flex:DisplayStyle.None;
            for(int i=0;i<feedLabels.Count;i++) feedLabels[i].text=i<feed.Count?feed[i]:"";
        }
    }
}
