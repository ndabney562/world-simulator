using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Runtime.InteropServices;
using UnityEngine;
using UnityEngine.Profiling;
using UnityEngine.Rendering;
using WorldSim.Bridge;

namespace WorldSim.Rendering
{
    /// <summary>Real58-footprint block, retained synthetic people and core-driven calendar/light.</summary>
    public sealed class SliceDemo : MonoBehaviour
    {
        public long Seed=71449;
        public int StartYear=2026,StartMonth=6,StartDay=22,StartHourUtc=11,StartMinuteUtc=22,StartSecondUtc=30;
        SliceHost host;
        readonly SliceFrame frame=new SliceFrame();
        SliceCamera orbit;
        SlicePicker picker;
        readonly SliceTapGesture tap=new SliceTapGesture();
        SliceInspection inspection;
        string inspectionText="";
        ulong consequenceRoot;
        Vector2 inspectionScroll;
        public SliceInspection CurrentInspection=>inspection;
        public string InspectorText=>inspectionText;
        public int InspectionQueries {get;private set;}
        CalendarPresentation calendar;
        Camera sceneCamera;
        GameObject staticRoot,groundRoot;
        Mesh staticMesh,groundMesh,capsuleMesh,barMesh;
        Material[] staticMaterials;
        Material groundMaterial,residentMaterial,aggregateMaterial;
        readonly Matrix4x4[] aggregateMatrices=new Matrix4x4[4];
        readonly SliceBuilding[] featured=new SliceBuilding[4];
        Bounds drawBounds;
        bool streetRequested,pointerUiCapture,saving;
        string notice="",status="",logPath="";
        StreamWriter log;
        double runStart,nextLog,nextStatus;
        int windowFrames;
        double frameSum,frameMax,fillSum,applySum,applyMax;
        ulong tickStart;
        int layoutWidth,layoutHeight;
        float fps,meanFrame,maxFrame,meanFill,meanApply,maxApply;
        public SliceFrame CurrentFrame => frame;
        public SliceGeometry Geometry => host?.Geometry;
        public SliceCamera Orbit => orbit;
        public Camera SceneCamera => sceneCamera;
        public string CsvPath => logPath;
        public double LastFillMs { get; private set; }
        public SliceAdvanceStats LastAdvance { get; private set; }
        public string CalendarText => calendar?.Text??"";
        public bool WaitingForStreet => streetRequested;
        static readonly Color[] FeaturedColours={Color.cyan,Color.green,Color.yellow,Color.magenta};

#if UNITY_WEBGL && !UNITY_EDITOR
        [DllImport("__Internal")] static extern void WorldSimFlushSave(string path,string receiver);
        [DllImport("__Internal")] static extern void WorldSimDownloadSave(string path);
        [DllImport("__Internal")] static extern void WorldSimImportSave(string path,string receiver);
        bool exportAfterFlush,browserFlushPending;
#endif
        void OnEnable()
        {
            try {Initialize();} catch {Release();throw;}
        }
        Material Material(string name,Color colour,bool instanced=false)
        {
            Shader shader=Shader.Find("Universal Render Pipeline/Lit");
            if(shader==null) throw new InvalidOperationException("Missing URP Lit shader.");
            return new Material(shader) {name=name,color=colour,enableInstancing=instanced};
        }
        void Initialize()
        {
            if(host!=null) return;
            sceneCamera=Camera.main;if(sceneCamera==null) throw new InvalidOperationException("Real block needs the scene camera.");
            Light sun=RenderSettings.sun;if(sun==null||sun.type!=LightType.Directional) throw new InvalidOperationException("Real block needs its directional sun light.");
            sun.shadows=LightShadows.None;calendar=new CalendarPresentation(sun);
            RenderSettings.ambientMode=AmbientMode.Flat;RenderSettings.ambientLight=new Color(0.8f,0.8f,0.8f);
            long start=NativeWorld.UtcToSim(StartYear,(uint)StartMonth,(uint)StartDay,(uint)StartHourUtc,(uint)StartMinuteUtc,(uint)StartSecondUtc);
            host=new SliceHost(unchecked((ulong)Seed),start);LastFillMs=host.Fill(frame,out var stats);LastAdvance=stats;
            BuildGeometry(host.Geometry);UpdateLayout();orbit=new SliceCamera(sceneCamera,host.Geometry.Bounds);
            capsuleMesh=ToyMeshes.Capsule();barMesh=ToyMeshes.Box();residentMaterial=Material("Real slice residents",new Color(1,0.3f,0),true);
            picker=new SlicePicker(host.Geometry,capsuleMesh);
            aggregateMaterial=Material("Coarse occupancy bars",new Color(0.15f,0.45f,1),true);
            drawBounds=host.Geometry.Bounds;drawBounds.Expand(30);Application.targetFrameRate=60;
            runStart=Time.realtimeSinceStartupAsDouble;nextLog=runStart+1;nextStatus=runStart+0.5;tickStart=frame.Tick;
            logPath=Path.Combine(Application.persistentDataPath,"vs008-real-"+Seed+"-"+DateTime.UtcNow.ToString("yyyyMMdd-HHmmss",CultureInfo.InvariantCulture)+".csv");
            log=new StreamWriter(logPath);log.WriteLine("elapsed_s,backend,seed,population,buildings,vertices,indices,tick,scale,fps,frame_mean_ms,frame_max_ms,fill_mean_ms,native_apply_per_tick_ms,native_apply_max_ms,l1,l2,local_stationary,in_transit,off_map,managed_mb,unity_mb,process_mb,thermal");log.Flush();
            calendar.Apply(frame.Sky);RefreshStatus();
        }
        void BuildGeometry(SliceGeometry geometry)
        {
            staticMaterials=new Material[5];staticMaterials[0]=Material("Open-data context",new Color(0.35f,0.4f,0.45f));
            for(int i=0;i<4;i++) staticMaterials[i+1]=Material("Featured building "+i,FeaturedColours[i]);
            var groups=new List<int>[5];for(int i=0;i<5;i++) groups[i]=new List<int>();
            foreach(var row in geometry.Buildings)
            {
                int group=0;
                if(row.Featured) for(int i=0;i<4;i++) if(geometry.FeaturedIds[i]==row.Id) {group=i+1;featured[i]=row;break;}
                for(int i=row.FirstIndex;i<row.FirstIndex+row.IndexCount;i++) groups[group].Add(geometry.Indices[i]);
            }
            staticMesh=new Mesh {name="Native58 footprint extrusions",vertices=geometry.Vertices,normals=geometry.Normals,subMeshCount=5};
            for(int i=0;i<5;i++) staticMesh.SetTriangles(groups[i],i,false);staticMesh.bounds=geometry.Bounds;
            staticRoot=new GameObject("Native real block massing");staticRoot.AddComponent<MeshFilter>().sharedMesh=staticMesh;
            var renderer=staticRoot.AddComponent<MeshRenderer>();renderer.sharedMaterials=staticMaterials;renderer.shadowCastingMode=ShadowCastingMode.Off;renderer.receiveShadows=false;
            Vector3 min=geometry.Bounds.min-Vector3.one*10,max=geometry.Bounds.max+Vector3.one*10;float up=geometry.Bounds.min.y-0.1f;
            groundMesh=new Mesh {name="Presentation ground plane",vertices=new[]{new Vector3(min.x,up,min.z),new Vector3(min.x,up,max.z),new Vector3(max.x,up,max.z),new Vector3(max.x,up,min.z)},normals=new[]{Vector3.up,Vector3.up,Vector3.up,Vector3.up},triangles=new[]{0,1,2,0,2,3}};
            groundMaterial=Material("Flat presentation ground",new Color(0.14f,0.16f,0.18f));groundRoot=new GameObject("Presentation ground plane");
            groundRoot.AddComponent<MeshFilter>().sharedMesh=groundMesh;var groundRenderer=groundRoot.AddComponent<MeshRenderer>();groundRenderer.sharedMaterial=groundMaterial;groundRenderer.shadowCastingMode=ShadowCastingMode.Off;groundRenderer.receiveShadows=false;
        }
        static float HudScale()=>Mathf.Max(1,Screen.width/1100f);
        void UpdateLayout()
        {
            if(layoutWidth==Screen.width&&layoutHeight==Screen.height) return;
            layoutWidth=Screen.width;layoutHeight=Screen.height;
            // Reserve the HUD strip so it cannot obscure featured footprints in
            // the default fit. Camera projection uses the remaining real viewport.
            float reserved=266*HudScale();
            sceneCamera.rect=new Rect(0,0,1,Mathf.Clamp((Screen.height-reserved)/Mathf.Max(1,Screen.height),0.25f,1));
            sceneCamera.ResetAspect();orbit?.ResizeIfNeeded();
        }
        bool WideHud()=>Screen.width/HudScale()>=900;
        Rect InspectorRect()=>new Rect(WideHud()?446:8,8,430,250);
        bool OverHud(Vector2 position)
        {
            Vector2 logical=new Vector2(position.x,Screen.height-position.y)/HudScale();
            return new Rect(8,8,430,250).Contains(logical)||(inspection!=null&&InspectorRect().Contains(logical));
        }
        bool PointerOverHud()
        {
            if(tap.UiCaptured||pointerUiCapture) return true;
            for(int i=0;i<Input.touchCount;i++) if(OverHud(Input.GetTouch(i).position)) return true;
            return OverHud(Input.mousePosition);
        }
        void ReadTap()
        {
            double now=Time.realtimeSinceStartupAsDouble;
            if(Input.touchCount==0&&!Input.GetMouseButton(0)) pointerUiCapture=false;
            for(int i=0;i<Input.touchCount;i++) if(Input.GetTouch(i).phase==TouchPhase.Began&&OverHud(Input.GetTouch(i).position)) pointerUiCapture=true;
            if(Input.GetMouseButtonDown(0)&&OverHud(Input.mousePosition)) pointerUiCapture=true;
            if(Input.touchCount>1) {tap.Cancel();return;}
            if(Input.touchCount==1)
            {
                var t=Input.GetTouch(0);Vector2 logical=t.position/HudScale();bool ui=OverHud(t.position);
                if(t.phase==TouchPhase.Began) tap.Begin(t.fingerId,logical,now,ui);
                else if(t.phase==TouchPhase.Moved||t.phase==TouchPhase.Stationary) tap.Move(t.fingerId,logical,ui);
                else if(t.phase==TouchPhase.Ended) {if(tap.End(t.fingerId,logical,now,ui)) InspectScreenPoint(t.position);}
                else tap.Cancel();
            }
            else
            {
                Vector2 position=Input.mousePosition,logical=position/HudScale();bool ui=OverHud(position);
                if(Input.GetMouseButtonDown(1)||Input.GetMouseButtonDown(2)) tap.Cancel();
                if(Input.GetMouseButtonDown(0)) tap.Begin(-1,logical,now,ui);
                if(Input.GetMouseButton(0)) tap.Move(-1,logical,ui);
                if(Input.GetMouseButtonUp(0)&&tap.End(-1,logical,now,ui)) InspectScreenPoint(position);
            }
        }
        public bool InspectScreenPoint(Vector2 position)
        {
            if(host==null||OverHud(position)||!sceneCamera.pixelRect.Contains(position)) return false;
            if(!picker.TryPick(sceneCamera.ScreenPointToRay(position),frame,out var hit)) {ClearInspection();return false;}
            if(hit.Id==0) {ClearInspection();notice="Source context BIN "+hit.Bin+"; no simulated population.";return false;}
            try
            {
                InspectionQueries++;var sample=host.Inspect(hit.Id);string text=SliceInspectionText.Format(sample);
                consequenceRoot=0;inspection=sample;inspectionText=text;inspectionScroll=Vector2.zero;notice="Tap again to refresh this snapshot.";return true;
            }
            catch(Exception error) {notice="Inspection failed: "+error.Message;return false;}
        }
        public void ClearInspection() {consequenceRoot=0;inspection=null;inspectionText="";inspectionScroll=Vector2.zero;}
        /// <summary>One explicit action; refresh keeps the same root and never reapplies illness.</summary>
        public bool InspectIllness()
        {
            if(host==null||inspection==null||inspection.Kind!=1) return false;
            try {
                var sample=host.Illness(inspection.Id,consequenceRoot);
                string text=SliceInspectionText.Consequences(sample);
                inspection=sample.Inspection;consequenceRoot=sample.Consequences.Root;inspectionText=text;inspectionScroll=Vector2.zero;
                notice="Tap the person to return to their details.";return true;
            } catch(Exception error) {notice="Illness action failed: "+error.Message;return false;}
        }
        void Update()
        {
            if(host==null) return;if(host.Failure!=null) throw new InvalidOperationException("Real slice worker stopped.",host.Failure);
            UpdateLayout();ReadTap();orbit.Update(PointerOverHud());
            if(streetRequested) TryFocusStreetResident();
            if(orbit.TryInterest(Time.realtimeSinceStartupAsDouble,out var target,out double radius)) host.SetCamera(target,radius);
            LastFillMs=host.Fill(frame,out var stats);LastAdvance=stats;calendar.Apply(frame.Sky);DrawInstances();
            double now=Time.realtimeSinceStartupAsDouble,frameMs=Time.unscaledDeltaTime*1000.0;
            windowFrames++;frameSum+=frameMs;frameMax=Math.Max(frameMax,frameMs);fillSum+=LastFillMs;applySum+=stats.ApplyTotalMs;applyMax=Math.Max(applyMax,stats.MaxApplyMs);
            if(now>=nextStatus)
            {
                meanFrame=(float)(frameSum/Math.Max(1,windowFrames));maxFrame=(float)frameMax;meanFill=(float)(fillSum/Math.Max(1,windowFrames));fps=meanFrame>0?1000/meanFrame:0;
                meanApply=frame.Tick==tickStart?0:(float)(applySum/(frame.Tick-tickStart));maxApply=(float)applyMax;RefreshStatus();
                windowFrames=0;frameSum=frameMax=fillSum=applySum=applyMax=0;tickStart=frame.Tick;nextStatus=now+0.5;
            }
            if(log!=null&&now>=nextLog) {WriteCsv(now);nextLog=now+1;}
            if(saving&&host!=null)
            {
                uint saveStatus=host.SaveStatus();
                if(saveStatus==0)
                {
#if UNITY_WEBGL && !UNITY_EDITOR
                    // Browser completion is delivered by IndexedDB, never by MEMFS alone.
                    if(!browserFlushPending)
                    {
                        browserFlushPending=true;notice="Saving to browser storage…";
                        WorldSimFlushSave(SavePath,gameObject.name);
                    }
#else
                    saving=false;
                    notice="Saved slice to "+Path.GetFileName(SavePath);
#endif
                }
                else if(saveStatus!=NativeSlice.SaveInProgress)
                {
                    saving=false;
                    notice="Background save failed (code "+saveStatus+")";
                }
            }
        }
        /// <summary>Only cached geometry and value/matrix arrays enter the warmed render path.</summary>
        public void DrawInstances()
        {
            if(host==null||residentMaterial==null) return;
            if(frame.RenderedResidents>0)
            {
                var parameters=new RenderParams(residentMaterial) {worldBounds=drawBounds,shadowCastingMode=ShadowCastingMode.Off,receiveShadows=false};
                Graphics.RenderMeshInstanced(parameters,capsuleMesh,0,frame.ResidentMatrices,frame.RenderedResidents);
            }
            int count=0;
            for(int i=0;i<4;i++)
            {
                var row=frame.Buildings[i];if(row.Mode==1||row.Occupancy==0) continue;
                var source=featured[i];float height=Mathf.Max(0.25f,row.Occupancy*0.25f);
                aggregateMatrices[count++]=Matrix4x4.TRS(new Vector3(source.Centroid.x,source.Height+height*0.5f+0.1f,source.Centroid.y),Quaternion.identity,new Vector3(0.6f,height,0.6f));
            }
            if(count>0)
            {
                var parameters=new RenderParams(aggregateMaterial) {worldBounds=drawBounds,shadowCastingMode=ShadowCastingMode.Off,receiveShadows=false};
                Graphics.RenderMeshInstanced(parameters,barMesh,0,aggregateMatrices,count);
            }
        }
        /// <summary>First request a real local view; then focus an actually published perimeter walker.</summary>
        public void RequestStreetView()
        {
            if(host==null) return;
            foreach(var row in host.Geometry.Buildings) if(row.Bin==3153706) {orbit.SetView(new Vector3(row.Centroid.x,0.85f,row.Centroid.y),150,25,60);break;}
            streetRequested=true;notice=frame.Scale==0?"Paused: resume x1 for nearby detail.":frame.Scale>10?"Select x1 or x10 for nearby detail.":"Waiting for a local walker…";
        }
        bool TryFocusStreetResident()
        {
            if(frame.Scale>10) return false;
            Bounds bounds=host.Geometry.Bounds;
            for(int i=0;i<frame.Population;i++)
            {
                var r=frame.Residents[i];if(r.Location!=1||r.Activity!=1) continue;
                Vector3 p=r.Foot;
                if(p.x>=bounds.min.x-1&&p.x<=bounds.max.x+1&&p.z>=bounds.min.z-1&&p.z<=bounds.max.z+1) continue;
                SliceBuilding node=default;for(int j=0;j<4;j++) if(featured[j].Id==r.Governing) {node=featured[j];break;}
                Vector3 toNode=new Vector3(node.Centroid.x-p.x,0,node.Centroid.y-p.z);float distance=toNode.magnitude;
                if(distance<0.001f||distance>30) continue;
                Vector3 direction=toNode/distance;
                // Keep the governing node inside the20m interest circle while
                // looking from outside the block toward the actual person.
                Vector3 target=p+direction*Mathf.Max(0,distance-19);target.y=0.85f;
                orbit.SetView(target,Mathf.Max(12,distance-19+8),Mathf.Atan2(direction.x,direction.z)*Mathf.Rad2Deg,8);
                streetRequested=false;notice="Street view: local resident "+r.Id;return true;
            }
            return false;
        }
        public void SetSpeed(uint scale)
        {
            if(host==null) throw new InvalidOperationException("No real slice.");host.SetScale(scale);
            if(streetRequested) notice=scale==0?"Paused: resume x1 for nearby detail.":scale>10?"Select x1 or x10 for nearby detail.":"Waiting for a local walker…";
        }
        public void ResetView() {streetRequested=false;notice="";orbit?.Reset();}
        public string SavePath => Path.Combine(Application.persistentDataPath,"slice-demo.wssave");
        public void SaveSlice()
        {
            if(host==null||saving) return;
            try
            {
                host.SaveBackground(SavePath);
                saving=true;
                notice="Saving slice in background…";
            }
            catch(Exception error)
            {
                notice="Save failed: "+error.Message;
            }
        }
        public void LoadSlice()
        {
            if(saving) {notice="Wait for the save to finish.";return;}
            if(!File.Exists(SavePath)) {notice="No saved world yet.";return;}
            ReplaceFromSave(SavePath,false);
        }
        void ReplaceFromSave(string path,bool imported)
        {
            SliceHost candidate=null;
            try
            {
                // Validate and prepare the replacement before releasing the live world.
                candidate=SliceHost.Load(path);
                var candidateFrame=new SliceFrame();candidate.Fill(candidateFrame,out _);
                var candidatePicker=new SlicePicker(candidate.Geometry,capsuleMesh);
                if(imported)
                {
                    candidate.Save(SavePath);
                    // Remove staging BEFORE syncfs enumerates the filesystem.
                    // Deleting it after sync starts races IDBFS's async reads.
                    File.Delete(path);
                }
                var previous=host;host=candidate;candidate=null;previous?.Dispose();
                ClearInspection();picker=candidatePicker;
                LastFillMs=host.Fill(frame,out var stats);LastAdvance=stats;
                calendar.Apply(frame.Sky);RefreshStatus();
                notice="Loaded slice (tick "+frame.Tick+")";
#if UNITY_WEBGL && !UNITY_EDITOR
                if(imported) {saving=true;browserFlushPending=true;notice="Saving to browser storage…";WorldSimFlushSave(SavePath,gameObject.name);}
#endif
            }
            catch(Exception error) {notice="Load failed: "+error.Message;}
            finally {candidate?.Dispose();}
        }
        // Called by the page toolbar; native HUD and persistence are unchanged.
        public void ExportBrowserSave()
        {
#if UNITY_WEBGL && !UNITY_EDITOR
            if(saving) {notice="Wait for the save to finish.";return;}
            exportAfterFlush=true;SaveSlice();
            if(!saving) exportAfterFlush=false;
#endif
        }
        public void ImportBrowserSave()
        {
#if UNITY_WEBGL && !UNITY_EDITOR
            if(saving) {notice="Wait for the save to finish.";return;}
            WorldSimImportSave(SavePath+".import",gameObject.name);
#endif
        }
        public void BrowserImportReady(string path)
        {
#if UNITY_WEBGL && !UNITY_EDITOR
            if(saving||path!=SavePath+".import") return;
            try {ReplaceFromSave(path,true);} finally {if(File.Exists(path)) File.Delete(path);}
#endif
        }
        public void BrowserSaveComplete(string error)
        {
#if UNITY_WEBGL && !UNITY_EDITOR
            saving=false;browserFlushPending=false;
            notice=error.Length==0?"Saved in this browser.":"Browser save failed; download a backup. "+error;
            if(exportAfterFlush)
            {
                exportAfterFlush=false;
                // An export is still available when storage is denied/full.
                WorldSimDownloadSave(SavePath);
            }
#endif
        }
        public void BrowserImportError(string error)
        {
#if UNITY_WEBGL && !UNITY_EDITOR
            notice="Import failed: "+error;
#endif
        }
        void RefreshStatus()
        {
            string process=PlatformStats.FootprintBytes==0?"n/a":(PlatformStats.FootprintBytes/1048576.0).ToString("F0",CultureInfo.InvariantCulture)+"MB";
            status=FormattableString.Invariant($"REAL BLOCK · seed{Seed} · {frame.Population} people ·58 footprints\n{fps:F0} fps · frame {meanFrame:F2}ms (max{maxFrame:F2}) · fill {meanFill:F3}ms\nNative apply/tick {meanApply:F4}ms (max call{maxApply:F3}) · tick{frame.Tick}\nL1 {frame.L1} · L2 {frame.L2} · local {frame.LocalStationary} · transit {frame.InTransit} · off-map {frame.OffMapStationary}\nMemory: managed {GC.GetTotalMemory(false)/1048576.0:F0}MB · process {process} · thermal {PlatformStats.ThermalName(PlatformStats.ThermalState)}");
        }
        void WriteCsv(double now)
        {
            var c=CultureInfo.InvariantCulture;
            log.WriteLine(string.Join(",",(now-runStart).ToString("F3",c),"real-slice",Seed,frame.Population,Geometry.Buildings.Length,Geometry.Vertices.Length,Geometry.Indices.Length,frame.Tick,frame.Scale,
                fps.ToString("F3",c),meanFrame.ToString("F3",c),maxFrame.ToString("F3",c),meanFill.ToString("F4",c),meanApply.ToString("F5",c),maxApply.ToString("F4",c),frame.L1,frame.L2,frame.LocalStationary,frame.InTransit,frame.OffMapStationary,
                (GC.GetTotalMemory(false)/1048576.0).ToString("F2",c),(Profiler.GetTotalAllocatedMemoryLong()/1048576.0).ToString("F2",c),PlatformStats.FootprintBytes==0?"":(PlatformStats.FootprintBytes/1048576.0).ToString("F2",c),PlatformStats.ThermalState));log.Flush();
        }
        void OnGUI()
        {
            if(host==null) return;Matrix4x4 previousMatrix=GUI.matrix;
            GUI.matrix=Matrix4x4.Scale(new Vector3(HudScale(),HudScale(),1));
            // The camera clears only the viewport below this strip. Clear the
            // entire strip opaquely so changing text never accumulates there.
            Color previous=GUI.color;GUI.color=Color.black;
            GUI.DrawTexture(new Rect(0,0,Screen.width/HudScale(),266),Texture2D.whiteTexture);GUI.color=previous;
            if(inspection==null||WideHud())
            {
                GUI.Box(new Rect(8,8,430,250),GUIContent.none);
                GUILayout.BeginArea(new Rect(14,12,418,242));GUILayout.Label(calendar.Text);GUILayout.Label(status);
                GUILayout.BeginHorizontal();if(GUILayout.Button("Pause")) SetSpeed(0);if(GUILayout.Button("x1")) SetSpeed(1);if(GUILayout.Button("x10")) SetSpeed(10);if(GUILayout.Button("x100")) SetSpeed(100);GUILayout.EndHorizontal();
                GUILayout.BeginHorizontal();if(GUILayout.Button("Overview")) ResetView();if(GUILayout.Button("Street")) RequestStreetView();if(GUILayout.Button("Save")) SaveSlice();if(GUILayout.Button("Load")) LoadSlice();GUILayout.EndHorizontal();
                GUILayout.Label("Tap: inspect · Drag: orbit · right/two-finger drag: pan · wheel/pinch: zoom\nBlue bars: coarse current occupants.");if(notice.Length>0) GUILayout.Label(notice);GUILayout.EndArea();
            }
            if(inspection!=null)
            {
                Rect panel=InspectorRect();GUI.Box(panel,GUIContent.none);
                GUILayout.BeginArea(new Rect(panel.x+6,panel.y+6,panel.width-12,panel.height-12));
                if(GUILayout.Button("Close inspection")) ClearInspection();
                else {if(inspection.Kind==1&&GUILayout.Button(consequenceRoot==0?"Curse with illness":"Refresh consequences")) InspectIllness();inspectionScroll=GUILayout.BeginScrollView(inspectionScroll);GUILayout.Label(inspectionText);GUILayout.EndScrollView();}
                GUILayout.EndArea();
            }
            GUI.matrix=previousMatrix;
        }
        void OnDisable() {Release();}
        void OnDestroy() {Release();}
        void Release()
        {
            ClearInspection();tap.Cancel();pointerUiCapture=false;picker=null;InspectionQueries=0;
            host?.Dispose();host=null;log?.Dispose();log=null;
            if(staticRoot!=null) DestroyImmediate(staticRoot);if(groundRoot!=null) DestroyImmediate(groundRoot);staticRoot=groundRoot=null;
            foreach(var mesh in new[]{staticMesh,groundMesh,capsuleMesh,barMesh}) if(mesh!=null) DestroyImmediate(mesh);staticMesh=groundMesh=capsuleMesh=barMesh=null;
            if(staticMaterials!=null) foreach(var material in staticMaterials) if(material!=null) DestroyImmediate(material);staticMaterials=null;
            if(groundMaterial!=null) DestroyImmediate(groundMaterial);if(residentMaterial!=null) DestroyImmediate(residentMaterial);if(aggregateMaterial!=null) DestroyImmediate(aggregateMaterial);
            groundMaterial=residentMaterial=aggregateMaterial=null;
        }
    }
}
