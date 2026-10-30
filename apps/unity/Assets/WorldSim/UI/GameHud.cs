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
        sealed class Watch {public ulong Subject,Root;public bool Done;}
        const int FeedLines=8;
        const double PollSeconds=1;
        static readonly Color Panel=new Color(0.07f,0.08f,0.11f,0.92f),Accent=new Color(0.95f,0.7f,0.25f),Text=new Color(0.92f,0.92f,0.9f),Dim=new Color(0.62f,0.64f,0.68f);
        SliceDemo demo;
        UIDocument document;
        PanelSettings settings;
        ThemeStyleSheet theme;
        VisualElement root,card,controls,feedPanel,powerBar,schedule;
        Label calendar,notice,stats,cardTitle,cardBody,feedEmpty,hint;
        Button curse,closeCard;
        readonly Dictionary<uint,Button> speeds=new Dictionary<uint,Button>();
        readonly List<Watch> watches=new List<Watch>();
        readonly HashSet<ulong> seen=new HashSet<ulong>();
        readonly Dictionary<ulong,string> names=new Dictionary<ulong,string>();
        readonly List<string> feed=new List<string>();
        readonly List<Label> feedLabels=new List<Label>();
        SliceInspection shown;
        bool armed,built;
        double nextPoll,warningUntil;
        string warning="";
        public bool PowerArmed=>armed;
        public IReadOnlyList<string> Feed=>feed;
        /// <summary>Select or deselect the power; the next resident tap casts it.</summary>
        public void TogglePower() {armed=!armed;}

        void OnEnable()
        {
            demo=GetComponent<SliceDemo>();
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
            watches.Clear();seen.Clear();names.Clear();feed.Clear();feedLabels.Clear();speeds.Clear();shown=null;armed=false;
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

            card=Box(strip,Panel);card.style.width=320;card.style.flexShrink=1;card.style.marginRight=8;
            var head=Row(card);head.style.justifyContent=Justify.SpaceBetween;head.style.flexWrap=Wrap.NoWrap;
            cardTitle=Line(head,"",Accent,16);closeCard=Action(head,"Close",demo.ClearInspection);closeCard.style.marginTop=0;
            cardBody=Line(card,"",Text,13);
            var scroll=new ScrollView(ScrollViewMode.Vertical);scroll.style.flexGrow=1;scroll.style.marginTop=4;card.Add(scroll);schedule=scroll.contentContainer;

            feedPanel=Box(strip,Panel);feedPanel.style.flexGrow=1;feedPanel.style.flexShrink=1;feedPanel.style.minWidth=200;
            Line(feedPanel,"What's happening",Accent,14);
            feedEmpty=Line(feedPanel,"Choose a power below, then tap a resident.",Dim,12);
            for(int i=0;i<FeedLines;i++) {var l=Line(feedPanel,"",Text,12);l.style.marginTop=2;feedLabels.Add(l);}

            // Power bar: one button per god power; select, then tap a resident.
            powerBar=new VisualElement();var p=powerBar.style;p.position=Position.Absolute;p.left=p.right=0;p.bottom=12;p.alignItems=Align.Center;powerBar.pickingMode=PickingMode.Ignore;root.Add(powerBar);
            hint=Line(powerBar,"",Accent,13);
            var bar=Box(powerBar,Panel);bar.style.flexDirection=FlexDirection.Row;
            curse=Action(bar,"Curse with illness",TogglePower);curse.style.fontSize=15;
            built=true;
        }
        void OnInspected(SliceInspection r)
        {
            if(!armed) return;
            if(r.Kind!=1) {warning="Curses need a person: tap a resident.";warningUntil=Time.realtimeSinceStartupAsDouble+3;return;}
            // Fresh tap sample has no root, so this applies the curse exactly once.
            if(demo.InspectIllness()) armed=false;
        }
        void OnCast(ulong subject,ulong root)
        {
            watches.Add(new Watch {Subject=subject,Root=root});nextPoll=0;
        }
        void OnWorldReplaced()
        {
            watches.Clear();seen.Clear();names.Clear();Add("Loaded a saved world.");
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
                try {c=demo.Consequences(w.Subject,w.Root);} catch(System.Exception) {w.Done=true;continue;}
                foreach(var e in c.Events)
                {
                    if(!seen.Add(e.Id)) continue;
                    string line=HudText.Feed(e,NameOf);if(line!=null) Add(line);
                }
                w.Done=HudText.Finished(c);
            }
            watches.RemoveAll(w=>w.Done);
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
            Refresh();
        }
        void Refresh()
        {
            calendar.text=demo.CalendarText;notice.text=demo.Notice;stats.text=demo.StatusText;
            uint current=demo.CurrentFrame.Scale;
            foreach(var pair in speeds) pair.Value.style.backgroundColor=pair.Key==current?new Color(0.55f,0.4f,0.12f):new Color(0.2f,0.22f,0.28f);
            curse.style.backgroundColor=armed?new Color(0.55f,0.12f,0.2f):new Color(0.2f,0.22f,0.28f);
            hint.text=Time.realtimeSinceStartupAsDouble<warningUntil?warning:armed?"Tap a resident to cast.":"";
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
            }
            card.style.display=inspection!=null?DisplayStyle.Flex:DisplayStyle.None;
            // Narrow screens: the card takes the feed's place while it is open.
            bool narrow=Screen.width/SliceDemo.HudUnits<900;
            feedPanel.style.display=narrow&&inspection!=null?DisplayStyle.None:DisplayStyle.Flex;
            controls.style.width=narrow?220:300;
            feedEmpty.style.display=feed.Count==0?DisplayStyle.Flex:DisplayStyle.None;
            for(int i=0;i<feedLabels.Count;i++) feedLabels[i].text=i<feed.Count?feed[i]:"";
        }
    }
}
