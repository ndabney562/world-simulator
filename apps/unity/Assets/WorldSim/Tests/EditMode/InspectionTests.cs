using System;
using System.IO;
using System.Collections;
using UnityEditor.SceneManagement;
using UnityEngine.SceneManagement;
using UnityEngine.TestTools;
using NUnit.Framework;
using Unity.Collections;
using UnityEngine;
using WorldSim.Bridge;
using WorldSim.Rendering;
using Object=UnityEngine.Object;

namespace WorldSim.Tests.EditMode
{
    public sealed class InspectionTests
    {
        static long Start=>NativeWorld.UtcToSim(2026,6,22,11,22,30);
        static byte[] Fixture(string name)=>ConvertHex(File.ReadAllText(Path.Combine(Application.dataPath,"../../../core/crates/ws-ffi/tests/fixtures/"+name+".hex")));
        static byte[] ConvertHex(string hex) {hex=hex.Replace("\n","").Replace("\r","").Replace(" ","");var b=new byte[hex.Length/2];for(int i=0;i<b.Length;i++) b[i]=Convert.ToByte(hex.Substring(i*2,2),16);return b;}
        static ulong Id(byte[] b)=>BitConverter.ToUInt64(b,40);
        static void U32(byte[] b,int at,uint value)=>Array.Copy(BitConverter.GetBytes(value),0,b,at,4);
        static void U64(byte[] b,int at,ulong value)=>Array.Copy(BitConverter.GetBytes(value),0,b,at,8);
        static SliceGeometry Geometry() {using(var b=new NativeArray<byte>(Fixture("slice-geometry-v6"),Allocator.Temp)) return SliceDecoder.DecodeGeometry(b,b.Length);}
        [Test]
        public void NativeInspectionMatchesSharedCoreFixturesAndEveryRenderedIdentityWithoutMutation()
        {
            using(var world=new NativeSlice(71449,Start))
            {
                world.SetCamera(Vector3.zero,500);world.Steps(20);var frame=new SliceFrame();world.Fill(frame,out _);
                ulong hash=frame.StateHash;
                foreach(string name in new[]{"inspect-resident-v6","inspect-building-v6"})
                {
                    byte[] raw=Fixture(name);SliceInspection expected;
                    using(var b=new NativeArray<byte>(raw,Allocator.Temp)) expected=InspectionDecoder.Decode(b,b.Length,world.Geometry,Id(raw));
                    var actual=world.Inspect(Id(raw));
                    Assert.That(actual.Tick,Is.EqualTo(expected.Tick));Assert.That(actual.Time,Is.EqualTo(expected.Time));Assert.That(actual.StateHash,Is.EqualTo(hash));
                    Assert.That(actual.Kind,Is.EqualTo(expected.Kind));Assert.That(actual.GivenName,Is.EqualTo(expected.GivenName));Assert.That(actual.FamilyName,Is.EqualTo(expected.FamilyName));
                    Assert.That(actual.Address,Is.EqualTo(expected.Address));Assert.That(actual.Bin,Is.EqualTo(expected.Bin));Assert.That(actual.Deficits,Is.EqualTo(expected.Deficits));
                    Assert.That(actual.Schedule,Is.EqualTo(expected.Schedule));Assert.That(actual.Events,Is.EqualTo(expected.Events));Assert.That(actual.Summary,Is.EqualTo(expected.Summary));
                    Assert.That(actual.StateHash,Is.EqualTo(actual.Sky.StateHash));Assert.That(actual.Tick,Is.EqualTo(actual.Sky.Tick));
                }
                for(int i=0;i<frame.Population;i++)
                {
                    var r=world.Inspect(frame.Residents[i].Id);Assert.That(r.Kind,Is.EqualTo(1));Assert.That(r.Detail,Is.EqualTo(frame.Residents[i].Detail));Assert.That(r.Activity,Is.EqualTo(frame.Residents[i].Activity));
                }
                for(int i=0;i<4;i++) Assert.That(world.Inspect(frame.Buildings[i].Id).Summary,Is.EqualTo(frame.Buildings[i]));
                Assert.Throws<InvalidOperationException>(()=>world.Inspect(0));world.Fill(frame,out _);Assert.That(frame.StateHash,Is.EqualTo(hash));Assert.That(frame.Tick,Is.EqualTo(20));
            }
        }
        [Test]
        public void MalformedInspectionNeverReplacesThePreviousCompleteRecord()
        {
            byte[] original=Fixture("inspect-resident-v6");var geometry=Geometry();SliceInspection published;
            using(var bytes=new NativeArray<byte>(original,Allocator.Temp)) published=InspectionDecoder.Decode(bytes,bytes.Length,geometry,Id(original));
            var saved=published;
            int state=96+(int)BitConverter.ToUInt32(original,64);
            foreach(Action<byte[]> damage in new Action<byte[]>[] {
                b=>U32(b,0,5),b=>U32(b,4,95),b=>U64(b,32,0),b=>U64(b,40,0),b=>U32(b,48,2),b=>U32(b,52,3),b=>U32(b,56,8193),b=>U32(b,60,2),
                b=>U32(b,64,uint.MaxValue),b=>U32(b,68,64),b=>U32(b,72,33),b=>U32(b,76,17),b=>U32(b,80,32),b=>U32(b,92,1),
                b=>U32(b,100,0),b=>U32(b,136,129),b=>b[140]=0xff,b=>U32(b,state,3),b=>U32(b,state+16,100001),b=>U64(b,state+56,1),b=>U32(b,state+76,2) })
            {
                byte[] raw=(byte[])original.Clone();damage(raw);
                using(var b=new NativeArray<byte>(raw,Allocator.Temp)) Assert.Catch<Exception>(()=>published=InspectionDecoder.Decode(b,b.Length,geometry,Id(original)));
                Assert.That(published,Is.SameAs(saved));
            }
            using(var b=new NativeArray<byte>(original,Allocator.Temp)) Assert.Catch<Exception>(()=>InspectionDecoder.Decode(b,b.Length-1,geometry,Id(original)));
            int scheduleAt=state+80,commute=-1;
            for(int i=0;i<BitConverter.ToUInt32(original,72);i++) if(BitConverter.ToUInt32(original,scheduleAt+i*48+16)!=0) {commute=scheduleAt+i*48;break;}
            Assert.That(commute,Is.GreaterThanOrEqualTo(0));
            foreach(Action<byte[]> damage in new Action<byte[]>[] {
                b=>U32(b,commute+20,BitConverter.ToUInt32(original,commute+20)+1),
                b=>{U64(b,commute+32,BitConverter.ToUInt64(original,commute+40));U32(b,commute+12,BitConverter.ToUInt32(original,commute+28)==uint.MaxValue?0u:1u);U32(b,commute+24,BitConverter.ToUInt32(original,commute+28));},
                b=>U32(b,state+4,(BitConverter.ToUInt32(original,state+4)+1)%6) })
            {
                byte[] raw=(byte[])original.Clone();damage(raw);
                using(var b=new NativeArray<byte>(raw,Allocator.Temp)) Assert.Catch<Exception>(()=>InspectionDecoder.Decode(b,b.Length,geometry,Id(original)));
            }
        }
        [Test]
        public void TapThresholdsCancelReturningDragsSecondTouchesUiAndLongPresses()
        {
            var tap=new SliceTapGesture();
            tap.Begin(1,Vector2.zero,0,false);Assert.That(tap.End(1,new Vector2(8,0),0.35,false),Is.True);
            tap.Begin(1,Vector2.zero,0,false);tap.Move(1,new Vector2(9,0),false);Assert.That(tap.End(1,Vector2.zero,0.2,false),Is.False);
            tap.Begin(1,Vector2.zero,0,false);tap.Cancel();Assert.That(tap.End(1,Vector2.zero,0.2,false),Is.False);
            tap.Begin(1,Vector2.zero,0,true);Assert.That(tap.UiCaptured,Is.True);Assert.That(tap.End(1,Vector2.zero,0.1,false),Is.False);
            tap.Begin(1,Vector2.zero,0,false);tap.Move(1,Vector2.one,true);Assert.That(tap.End(1,Vector2.zero,0.1,false),Is.False);
            tap.Begin(1,Vector2.zero,0,false);Assert.That(tap.End(1,Vector2.zero,0.351,false),Is.False);
            tap.Begin(1,Vector2.zero,1,false);Assert.That(tap.End(1,Vector2.zero,0,false),Is.False);
            tap.Begin(1,Vector2.zero,0,false);Assert.That(tap.End(1,new Vector2(float.NaN,0),0.1,false),Is.False);
        }
        [UnityTest]
        public IEnumerator RealSceneTapsInspectBuildingAndResidentAndNeverPollTheSelection()
        {
            yield return new EnterPlayMode();
            var loading=EditorSceneManager.LoadSceneAsyncInPlayMode("Assets/WorldSim/Scenes/RealBlock.unity",new LoadSceneParameters(LoadSceneMode.Single));
            while(!loading.isDone) yield return null;
            var demo=Object.FindFirstObjectByType<SliceDemo>();Assert.That(demo,Is.Not.Null);demo.SetSpeed(0);yield return null;
            var mesh=ToyMeshes.Capsule();var picker=new SlicePicker(demo.Geometry,mesh);bool selected=false;
            foreach(var row in demo.Geometry.Buildings)
            {
                if(!row.Featured) continue;
                for(int i=row.FirstIndex;i<row.FirstIndex+row.IndexCount;i+=3)
                {
                    Vector3 centre=(demo.Geometry.Vertices[demo.Geometry.Indices[i]]+demo.Geometry.Vertices[demo.Geometry.Indices[i+1]]+demo.Geometry.Vertices[demo.Geometry.Indices[i+2]])/3;
                    Vector3 point=demo.SceneCamera.WorldToScreenPoint(centre);if(point.z<=0) continue;
                    if(!picker.TryPick(demo.SceneCamera.ScreenPointToRay(point),demo.CurrentFrame,out var hit)||hit.Resident||hit.Id==0) continue;
                    if(!demo.InspectScreenPoint(point)) continue;
                    Assert.That(demo.CurrentInspection.Id,Is.EqualTo(hit.Id));Assert.That(demo.CurrentInspection.Kind,Is.EqualTo(2));selected=true;break;
                }
                if(selected) break;
            }
            Assert.That(selected,Is.True,"A visible featured building must be inspectable in the actual scene.");
            int calls=demo.InspectionQueries;ulong sampledTick=demo.CurrentInspection.Tick;string text=demo.InspectorText;
            for(int i=0;i<6;i++) yield return null;
            Assert.That(demo.InspectionQueries,Is.EqualTo(calls));Assert.That(demo.CurrentInspection.Tick,Is.EqualTo(sampledTick));Assert.That(demo.InspectorText,Is.EqualTo(text));
            File.WriteAllText(Path.Combine(Path.GetTempPath(),"worldsim73-building-inspection.txt"),text);
            UnityEditorInternal.InternalEditorUtility.RepaintAllViews();
            yield return new WaitForSecondsRealtime(0.5f);
            ScreenCapture.CaptureScreenshot(Path.Combine(Path.GetTempPath(),"worldsim73-building-inspection.png"));
            for(int i=0;i<3;i++) yield return null;
            demo.ClearInspection();demo.SetSpeed(1);demo.RequestStreetView();double deadline=Time.realtimeSinceStartupAsDouble+8;
            while(demo.WaitingForStreet&&Time.realtimeSinceStartupAsDouble<deadline) yield return null;
            Assert.That(demo.WaitingForStreet,Is.False);demo.SetSpeed(0);yield return null;selected=false;
            for(int i=0;i<demo.CurrentFrame.RenderedResidents;i++)
            {
                Vector3 centre=demo.CurrentFrame.ResidentMatrices[i].MultiplyPoint3x4(Vector3.zero),point=demo.SceneCamera.WorldToScreenPoint(centre);
                if(point.z<=0||!picker.TryPick(demo.SceneCamera.ScreenPointToRay(point),demo.CurrentFrame,out var hit)||!hit.Resident) continue;
                if(!demo.InspectScreenPoint(point)) continue;
                Assert.That(demo.CurrentInspection.Id,Is.EqualTo(hit.Id));Assert.That(demo.CurrentInspection.Kind,Is.EqualTo(1));selected=true;break;
            }
            Assert.That(selected,Is.True,"An actually visible local fine resident must be inspectable.");
            Assert.That(demo.InspectorText,Does.Contain("synthetic resident"));Assert.That(demo.InspectorText,Does.Contain("Need deficits"));
            File.WriteAllText(Path.Combine(Path.GetTempPath(),"worldsim73-resident-inspection.txt"),demo.InspectorText);
            UnityEditorInternal.InternalEditorUtility.RepaintAllViews();
            yield return new WaitForSecondsRealtime(0.5f);
            ScreenCapture.CaptureScreenshot(Path.Combine(Path.GetTempPath(),"worldsim73-resident-inspection.png"));
            for(int i=0;i<3;i++) yield return null;
            calls=demo.InspectionQueries;Assert.That(demo.InspectScreenPoint(new Vector2(10,Screen.height-10)),Is.False);Assert.That(demo.InspectionQueries,Is.EqualTo(calls));
            demo.enabled=false;Assert.That(demo.CurrentInspection,Is.Null);Assert.That(demo.InspectorText,Is.Empty);Object.DestroyImmediate(mesh);
            yield return new ExitPlayMode();
        }
        [Test]
        public void ContextOccludesHiddenCapsulesAndEqualDepthUsesStableResidentIdentity()
        {
            var geometry=Geometry();var mesh=ToyMeshes.Capsule();
            try
            {
                var picker=new SlicePicker(geometry,mesh);var frame=new SliceFrame();Vector3 roof=default;bool found=false;
                foreach(var row in geometry.Buildings)
                {
                    if(row.Featured) continue;
                    for(int i=row.FirstIndex;i<row.FirstIndex+row.IndexCount;i+=3)
                    {
                        int a=geometry.Indices[i],b=geometry.Indices[i+1],c=geometry.Indices[i+2];
                        if(geometry.Normals[a].y<0.9f) continue;roof=(geometry.Vertices[a]+geometry.Vertices[b]+geometry.Vertices[c])/3;found=true;break;
                    }
                    if(found) break;
                }
                Assert.That(found,Is.True);var ray=new Ray(roof+Vector3.up*200,Vector3.down);
                frame.RenderedResidents=1;frame.RenderedResidentIds[0]=100;frame.ResidentMatrices[0]=Matrix4x4.TRS(roof-Vector3.up*3,Quaternion.identity,Vector3.one);
                Assert.That(picker.TryPick(ray,frame,out var hit),Is.True);Assert.That(hit.Id,Is.Zero,"A context footprint must occlude the hidden resident.");
                frame.ResidentMatrices[0]=Matrix4x4.TRS(roof+Vector3.up*100,Quaternion.identity,Vector3.one);
                Assert.That(picker.TryPick(ray,frame,out hit),Is.True);Assert.That(hit.Resident,Is.True);Assert.That(hit.Id,Is.EqualTo(100));
                frame.RenderedResidents=2;frame.RenderedResidentIds[1]=99;frame.ResidentMatrices[1]=frame.ResidentMatrices[0];
                Assert.That(picker.TryPick(ray,frame,out hit),Is.True);Assert.That(hit.Id,Is.EqualTo(99));
                // Nonuniform capsule scale must not normalize away world-ray t.
                frame.RenderedResidents=1;frame.RenderedResidentIds[0]=100;
                var nearRoof=new Ray(roof+Vector3.up*10,Vector3.down);
                frame.ResidentMatrices[0]=Matrix4x4.TRS(roof+Vector3.up*0.5f,Quaternion.identity,new Vector3(0.5f,0.85f,0.5f));
                Assert.That(picker.TryPick(nearRoof,frame,out hit),Is.True);Assert.That(hit.Resident,Is.True,"The scaled capsule cap is in front of the roof.");
                frame.ResidentMatrices[0]=Matrix4x4.TRS(roof-Vector3.up*3,Quaternion.identity,Vector3.one*2);
                Assert.That(picker.TryPick(nearRoof,frame,out hit),Is.True);Assert.That(hit.Resident,Is.False,"The enlarged capsule is fully hidden behind the roof.");
                frame.RenderedResidents=0;Assert.That(picker.TryPick(ray,frame,out hit),Is.True);Assert.That(hit.Resident,Is.False);
                Assert.That(picker.TryPick(new Ray(Vector3.zero,Vector3.zero),frame,out _),Is.False);
            }
            finally {Object.DestroyImmediate(mesh);}
        }
    }
}
