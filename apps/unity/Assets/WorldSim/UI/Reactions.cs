using System.Collections.Generic;
using UnityEngine;
using UnityEngine.Rendering;
using WorldSim.Bridge;
using WorldSim.Rendering;

namespace WorldSim.UI
{
    /// <summary>
    /// Visible reactions (GP-003): an effect where each power lands, a status icon above every
    /// affected resident, a camera ease to the target on cast and an optional follow mode.
    /// Presentation only: reads the frame and the HUD's polled consequence rows, never writes
    /// simulation state. Warmed drawing reuses fixed matrix arrays and allocates nothing.
    /// </summary>
    [RequireComponent(typeof(SliceDemo))]
    public sealed class Reactions : MonoBehaviour
    {
        struct Effect {public EffectKind Kind;public Vector3 At;public double Start;}
        const int MaxEffects=16,Sparks=6,RainStreaks=120,Icons=6,ClusterColumns=4;
        const float HeadClearance=2.3f,RoofClearance=1.4f,ClusterSpacing=0.9f,ResidentZoom=60,BuildingZoom=90;
        static readonly Color[] IconColours=
        {
            Color.clear,new Color(0.55f,0.95f,0.55f),new Color(0.35f,0.65f,1),new Color(1,0.8f,0.25f),new Color(0.62f,0.7f,0.85f),new Color(0.95f,0.18f,0.18f)
        };
        SliceDemo demo;GameHud hud;
        readonly ResidentStatus status=new ResidentStatus();
        readonly Effect[] effects=new Effect[MaxEffects];int effectCount;
        Mesh box,ring;
        readonly Material[] iconMaterials=new Material[Icons];
        Material blessMaterial,curseMaterial,ringMaterial,cloudMaterial,rainMaterial;
        readonly Matrix4x4[][] iconMatrices=new Matrix4x4[Icons][];
        readonly int[] iconCounts=new int[Icons];
        readonly Matrix4x4[] blessMatrices=new Matrix4x4[MaxEffects*(Sparks+1)],curseMatrices=new Matrix4x4[MaxEffects*(Sparks+1)],ringMatrices=new Matrix4x4[MaxEffects];
        readonly Matrix4x4[] cloudMatrices=new Matrix4x4[3],rainMatrices=new Matrix4x4[RainStreaks];
        readonly Vector3[] rainSpots=new Vector3[RainStreaks];
        readonly Dictionary<ulong,SliceBuilding> buildings=new Dictionary<ulong,SliceBuilding>();
        readonly Dictionary<ulong,int> clusterSlots=new Dictionary<ulong,int>();
        SliceGeometry indexed;
        Bounds drawBounds;
        ulong following;
        /// <summary>Statuses shown now, for tests.</summary>
        public ResidentStatus Status=>status;
        /// <summary>The resident the camera follows, or 0.</summary>
        public ulong Following=>following;
        public int ActiveEffects=>effectCount;
        /// <summary>Icons submitted by the last draw, for tests.</summary>
        public int DrawnIcons {get;private set;}
        public bool StormVisible {get;private set;}

        static Material Lit(string name,Color colour)
        {
            Shader shader=Shader.Find("Universal Render Pipeline/Lit");
            if(shader==null) throw new System.InvalidOperationException("Missing URP Lit shader.");
            return new Material(shader) {name=name,color=colour,enableInstancing=true};
        }
        void OnEnable()
        {
            demo=GetComponent<SliceDemo>();hud=GetComponent<GameHud>();
            box=ToyMeshes.Box();ring=ToyMeshes.Ring();
            for(int i=1;i<Icons;i++) {iconMaterials[i]=Lit("Status icon "+(StatusIcon)i,IconColours[i]);iconMatrices[i]=new Matrix4x4[SliceFrame.MaxPopulation];}
            blessMaterial=Lit("Bless glow",new Color(1,0.85f,0.35f));curseMaterial=Lit("Curse puff",new Color(0.35f,0.8f,0.25f));
            ringMaterial=Lit("Gathering ring",new Color(1,0.75f,0.3f));cloudMaterial=Lit("Storm cloud",new Color(0.22f,0.24f,0.28f));rainMaterial=Lit("Rain",new Color(0.55f,0.7f,0.95f));
            if(hud!=null) {hud.ConsequenceSeen+=OnConsequence;hud.Casted+=OnCasted;}
            demo.WorldReplaced+=OnWorldReplaced;
        }
        void OnDisable()
        {
            if(hud!=null) {hud.ConsequenceSeen-=OnConsequence;hud.Casted-=OnCasted;}
            if(demo!=null) demo.WorldReplaced-=OnWorldReplaced;
            if(box!=null) Destroy(box);if(ring!=null) Destroy(ring);box=ring=null;
            for(int i=1;i<Icons;i++) if(iconMaterials[i]!=null) Destroy(iconMaterials[i]);
            foreach(var m in new[]{blessMaterial,curseMaterial,ringMaterial,cloudMaterial,rainMaterial}) if(m!=null) Destroy(m);
            blessMaterial=curseMaterial=ringMaterial=cloudMaterial=rainMaterial=null;
            status.Clear();effectCount=0;following=0;indexed=null;buildings.Clear();
        }
        void OnConsequence(InspectionEvent e,SliceConsequences c) {status.Apply(e,c.RootTag,c.End);}
        void OnWorldReplaced() {status.Clear();effectCount=0;following=0;}
        void OnCasted(ushort tag,ulong subject)
        {
            Index();if(indexed==null) return;
            double now=Time.realtimeSinceStartupAsDouble;var orbit=demo.Orbit;
            if(tag==HudCasts.StormTag) {status.BeginStorm(demo.CurrentFrame.SimTimeMicroseconds+(long)GameHud.StormUs);return;}
            if(tag==HudCasts.GatherTag)
            {
                if(!buildings.TryGetValue(subject,out var home)) return;
                var at=new Vector3(home.Centroid.x,indexed.Bounds.min.y+0.05f,home.Centroid.y);
                AddEffect(EffectKind.Gathering,at,now);orbit?.EaseTo(new Vector3(at.x,0.85f,at.z),Mathf.Min(orbit.Distance,BuildingZoom),now);
                return;
            }
            if(!Locate(subject,out var where)) return;
            AddEffect(tag==HudCasts.BlessTag?EffectKind.Bless:EffectKind.Curse,where,now);
            orbit?.EaseTo(new Vector3(where.x,0.85f,where.z),Mathf.Min(orbit.Distance,ResidentZoom),now);
        }
        /// <summary>Follow a resident (0 stops). Following eases there first, then tracks every frame.</summary>
        public void Follow(ulong id)
        {
            following=id;if(id==0||demo.Orbit==null||!Locate(id,out var where)) return;
            demo.Orbit.EaseTo(new Vector3(where.x,0.85f,where.z),Mathf.Min(demo.Orbit.Distance,ResidentZoom),Time.realtimeSinceStartupAsDouble);
        }
        void AddEffect(EffectKind kind,Vector3 at,double now)
        {
            // Oldest first: a full list drops the effect closest to finishing.
            if(effectCount==MaxEffects) {System.Array.Copy(effects,1,effects,0,MaxEffects-1);effectCount--;}
            effects[effectCount++]=new Effect {Kind=kind,At=at,Start=now};
        }
        void Index()
        {
            var geometry=demo.Geometry;if(geometry==null||ReferenceEquals(geometry,indexed)) return;
            indexed=geometry;buildings.Clear();
            foreach(var row in geometry.Buildings) if(row.Featured) buildings[row.Id]=row;
            drawBounds=geometry.Bounds;drawBounds.Expand(60);
            var b=geometry.Bounds;uint h=2166136261;
            for(int i=0;i<RainStreaks;i++)
            {
                // Fixed per-streak spots and phases from an integer hash: steady, repeatable rain.
                h=(h^(uint)i)*16777619;float u=(h&1023)/1023f;h=(h^0x9e37u)*16777619;float v=(h&1023)/1023f;h=(h^0x7f4au)*16777619;
                rainSpots[i]=new Vector3(Mathf.Lerp(b.min.x,b.max.x,u),(h&1023)/1024f,Mathf.Lerp(b.min.z,b.max.z,v));
            }
        }
        Vector3 Roof(ulong building,int slot)
        {
            if(!buildings.TryGetValue(building,out var row)) return new Vector3(float.NaN,0,0);
            float x=(slot%ClusterColumns-(ClusterColumns-1)*0.5f)*ClusterSpacing,z=(slot/ClusterColumns)*ClusterSpacing;
            return new Vector3(row.Centroid.x+x,row.Height+RoofClearance,row.Centroid.y+z);
        }
        /// <summary>A resident's current point: their feet when drawn locally, otherwise their home's roof.</summary>
        bool Locate(ulong id,out Vector3 where)
        {
            var frame=demo.CurrentFrame;where=default;
            for(int i=0;i<frame.Population;i++)
            {
                var r=frame.Residents[i];if(r.Id!=id) continue;
                if(r.Location==1) {where=r.Foot;return true;}
                where=Roof(r.Governing,0);return !float.IsNaN(where.x);
            }
            return false;
        }
        void LateUpdate()
        {
            if(demo==null||demo.Geometry==null||box==null) return;
            Index();double now=Time.realtimeSinceStartupAsDouble;var orbit=demo.Orbit;
            if(following!=0&&(orbit==null||orbit.UserMoved||demo.CurrentInspection==null||demo.CurrentInspection.Id!=following)) following=0;
            if(orbit!=null)
            {
                if(orbit.Easing) orbit.StepEase(now,ReactionMath.Ease,ReactionMath.EaseSeconds);
                else if(following!=0&&Locate(following,out var where)) orbit.Track(where);
            }
            Draw(now);
        }
        /// <summary>Submit icons and effects for this frame. Public so tests can check warm allocation.</summary>
        public void Draw(double now)
        {
            if(box==null||indexed==null) return;
            var frame=demo.CurrentFrame;long sim=frame.SimTimeMicroseconds;
            for(int i=1;i<Icons;i++) iconCounts[i]=0;
            clusterSlots.Clear();float bob=ReactionMath.Bob(now);
            var spin=Quaternion.Euler(45,(float)(now*90%360),45);var size=Vector3.one*0.45f;
            for(int i=0;i<frame.Population;i++)
            {
                var r=frame.Residents[i];var icon=status.Get(r.Id,sim,out ulong building);if(icon==StatusIcon.None) continue;
                Vector3 at;
                if(r.Location==1) at=r.Foot+Vector3.up*HeadClearance;
                else
                {
                    ulong place=building!=0?building:r.Governing;
                    clusterSlots.TryGetValue(place,out int slot);clusterSlots[place]=slot+1;
                    at=Roof(place,slot);if(float.IsNaN(at.x)) continue;
                }
                at.y+=bob;int k=(int)icon;iconMatrices[k][iconCounts[k]++]=Matrix4x4.TRS(at,spin,size);
            }
            int drawn=0;
            for(int k=1;k<Icons;k++) {drawn+=iconCounts[k];Submit(iconMaterials[k],box,iconMatrices[k],iconCounts[k]);}
            DrawnIcons=drawn;
            int live=0,bless=0,curse=0,rings=0;
            for(int i=0;i<effectCount;i++)
            {
                var e=effects[i];double age=now-e.Start;if(!ReactionMath.Alive(e.Kind,age)) continue;
                effects[live++]=e;float s=ReactionMath.Size(e.Kind,age);
                if(e.Kind==EffectKind.Gathering)
                {
                    float radius=ReactionMath.RingRadius(age);
                    ringMatrices[rings++]=Matrix4x4.TRS(e.At,Quaternion.identity,new Vector3(radius,1,radius));
                    continue;
                }
                if(e.Kind==EffectKind.Bless)
                {
                    // A rising gold column with sparks spiralling up around it.
                    blessMatrices[bless++]=Matrix4x4.TRS(e.At+Vector3.up*(1.6f*s),Quaternion.identity,new Vector3(0.9f*s,1.6f*s,0.9f*s));
                    for(int j=0;j<Sparks;j++)
                    {
                        float angle=(j*60+(float)age*180)*Mathf.Deg2Rad;
                        blessMatrices[bless++]=Matrix4x4.TRS(e.At+new Vector3(Mathf.Cos(angle)*0.8f,0.3f+(float)age*2,Mathf.Sin(angle)*0.8f),Quaternion.identity,Vector3.one*0.18f);
                    }
                }
                else
                {
                    // A green puff that swells and shrinks, shedding bits outward.
                    curseMatrices[curse++]=Matrix4x4.TRS(e.At+Vector3.up*1.2f,Quaternion.Euler(0,(float)(age*120),0),Vector3.one*(1.4f*s));
                    for(int j=0;j<Sparks;j++)
                    {
                        float angle=j*60*Mathf.Deg2Rad,reach=0.5f+(float)age*1.5f;
                        curseMatrices[curse++]=Matrix4x4.TRS(e.At+new Vector3(Mathf.Cos(angle)*reach,1.2f,Mathf.Sin(angle)*reach),Quaternion.identity,Vector3.one*(0.3f*Mathf.Max(0.2f,s)));
                    }
                }
            }
            effectCount=live;
            Submit(blessMaterial,box,blessMatrices,bless);Submit(curseMaterial,box,curseMatrices,curse);Submit(ringMaterial,ring,ringMatrices,rings);
            StormVisible=status.StormActive(sim);
            if(StormVisible)
            {
                var b=indexed.Bounds;float ground=b.min.y,top=b.max.y+8;
                for(int i=0;i<3;i++)
                {
                    float shift=(i-1)*b.size.x*0.22f;
                    cloudMatrices[i]=Matrix4x4.TRS(new Vector3(b.center.x+shift,top+1.5f+(i%2)*1.2f,b.center.z+(i-1)*b.size.z*0.1f),Quaternion.identity,new Vector3(b.size.x*0.45f,2.5f,b.size.z*0.6f));
                }
                Submit(cloudMaterial,box,cloudMatrices,3);
                for(int i=0;i<RainStreaks;i++)
                {
                    var spot=rainSpots[i];float y=ground+ReactionMath.RainY(now,spot.y)/ReactionMath.RainHeight*(top-ground);
                    rainMatrices[i]=Matrix4x4.TRS(new Vector3(spot.x,y,spot.z),Quaternion.identity,new Vector3(0.05f,1.1f,0.05f));
                }
                Submit(rainMaterial,box,rainMatrices,RainStreaks);
            }
        }
        void Submit(Material material,Mesh mesh,Matrix4x4[] matrices,int count)
        {
            if(count==0) return;
            var parameters=new RenderParams(material) {worldBounds=drawBounds,shadowCastingMode=ShadowCastingMode.Off,receiveShadows=false};
            Graphics.RenderMeshInstanced(parameters,mesh,0,matrices,count);
        }
    }
}
