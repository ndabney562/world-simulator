using System;
using System.Collections;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using NUnit.Framework;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.Rendering.Universal;
using UnityEngine.SceneManagement;
using UnityEngine.TestTools;
using WorldSim.Rendering;
using Object=UnityEngine.Object;

namespace WorldSim.Tests.EditMode
{
    public sealed class SlicePerformanceTests
    {
        [Serializable] sealed class Measurement
        {
            public uint scale;
            public int frames,population,buildings,vertices,indices,width,height;
            public double seconds,fps,ticks_per_second,mean_frame_ms,p95_frame_ms,max_frame_ms,mean_fill_ms,mean_render_fence_ms,native_apply_per_tick_ms,max_native_apply_ms;
            public ulong first_tick,last_tick;
        }
        [Serializable] sealed class Summary {public string backend="real-slice";public Measurement[] cases;}
        static double Seconds()=>System.Diagnostics.Stopwatch.GetTimestamp()/(double)System.Diagnostics.Stopwatch.Frequency;
        static void Fence(Camera camera,RenderTexture target,Texture2D pixel)
        {
            RenderPipeline.SubmitRenderRequest(camera,new UniversalRenderPipeline.SingleCameraRequest {destination=target});
            RenderTexture previous=RenderTexture.active;RenderTexture.active=target;pixel.ReadPixels(new Rect(0,0,1,1),0,0);RenderTexture.active=previous;
        }
        [UnityTest]
        public IEnumerator RealSliceThirtyPeopleHasFencedEditorFrameAndWorkerEvidenceAtOneAndHundredTimes()
        {
            yield return new EnterPlayMode();yield return Measure();yield return new ExitPlayMode();
        }
        static IEnumerator Measure()
        {
            var loading=EditorSceneManager.LoadSceneAsyncInPlayMode("Assets/WorldSim/Scenes/RealBlock.unity",new LoadSceneParameters(LoadSceneMode.Single));
            while(!loading.isDone) yield return null;
            var demo=Object.FindFirstObjectByType<SliceDemo>();Assert.That(demo,Is.Not.Null);Assert.That(demo.CurrentFrame.Population,Is.EqualTo(30));
            var camera=demo.SceneCamera;RenderTexture previous=camera.targetTexture;Rect previousRect=camera.rect;int previousRate=Application.targetFrameRate;
            var target=new RenderTexture(960,540,24,RenderTextureFormat.ARGB32);var pixel=new Texture2D(1,1,TextureFormat.RGB24,false);
            string csv=Path.Combine(Path.GetTempPath(),"worldsim72-editor-frames.csv");var results=new List<Measurement>(2);
            try
            {
                target.Create();camera.targetTexture=target;camera.rect=new Rect(0,0,1,1);camera.aspect=16f/9;Application.targetFrameRate=-1;
                demo.RequestStreetView();double timeout=Seconds()+8;while(demo.WaitingForStreet&&Seconds()<timeout) yield return null;
                Assert.That(demo.WaitingForStreet,Is.False);
                for(int i=0;i<10;i++) demo.DrawInstances();long allocation=GC.GetAllocatedBytesForCurrentThread();for(int i=0;i<10;i++) demo.DrawInstances();
                Assert.That(GC.GetAllocatedBytesForCurrentThread()-allocation,Is.Zero,"Warmed render submission must reuse matrices/materials.");
                using(var writer=new StreamWriter(csv))
                {
                    writer.WriteLine("host_seconds,backend,scale,population,buildings,vertices,indices,tick,state_hash,viewport_width,viewport_height,frame_ms,fill_ms,render_fence_ms,native_apply_total_ms,native_apply_max_ms,l1,l2");
                    foreach(uint speed in new uint[]{1,100})
                    {
                        demo.SetSpeed(speed);double warm=Seconds()+2;
                        while(Seconds()<warm) {yield return null;Fence(camera,target,pixel);}
                        Assert.That(camera.rect,Is.EqualTo(new Rect(0,0,1,1)));
                        Assert.That(camera.pixelWidth,Is.EqualTo(960));Assert.That(camera.pixelHeight,Is.EqualTo(540));
                        var durations=new List<double>(16384);double fill=0,render=0,apply=0,maxApply=0;
                        ulong first=demo.CurrentFrame.Tick;double begin=Seconds(),previousFrame=begin;
                        while(Seconds()-begin<10)
                        {
                            yield return null;double renderBegin=Seconds();Fence(camera,target,pixel);double now=Seconds();double frame=(now-previousFrame)*1000;previousFrame=now;
                            double renderMs=(now-renderBegin)*1000;durations.Add(frame);fill+=demo.LastFillMs;render+=renderMs;apply+=demo.LastAdvance.ApplyTotalMs;maxApply=Math.Max(maxApply,demo.LastAdvance.MaxApplyMs);
                            var f=demo.CurrentFrame;
                            writer.WriteLine(FormattableString.Invariant($"{now:F9},real-slice,{speed},30,58,{demo.Geometry.Vertices.Length},{demo.Geometry.Indices.Length},{f.Tick},{f.StateHash},960,540,{frame:F6},{demo.LastFillMs:F6},{renderMs:F6},{demo.LastAdvance.ApplyTotalMs:F6},{demo.LastAdvance.MaxApplyMs:F6},{f.L1},{f.L2}"));
                        }
                        double elapsed=previousFrame-begin;ulong last=demo.CurrentFrame.Tick;durations.Sort();double sum=0;foreach(double value in durations) sum+=value;
                        Assert.That(camera.rect,Is.EqualTo(new Rect(0,0,1,1)));
                        var result=new Measurement {scale=speed,frames=durations.Count,population=30,buildings=58,vertices=demo.Geometry.Vertices.Length,indices=demo.Geometry.Indices.Length,width=960,height=540,
                            seconds=elapsed,fps=durations.Count/elapsed,ticks_per_second=(last-first)/elapsed,mean_frame_ms=sum/durations.Count,p95_frame_ms=durations[(int)(durations.Count*0.95)],max_frame_ms=durations[durations.Count-1],
                            mean_fill_ms=fill/durations.Count,mean_render_fence_ms=render/durations.Count,native_apply_per_tick_ms=last==first?0:apply/(last-first),max_native_apply_ms=maxApply,first_tick=first,last_tick=last};
                        results.Add(result);writer.Flush();
                        Debug.Log(FormattableString.Invariant($"VS008 actual real-slice30 scale{speed}: fps={result.fps:F3}, ticks/s={result.ticks_per_second:F3}, full960x540 fenced camera, csv={csv}"));
                        Assert.That(result.fps,Is.GreaterThanOrEqualTo(60),"Desktop engineering check; actual iPhone evidence remains outstanding.");
                        Assert.That(result.ticks_per_second,Is.InRange(speed*10*0.98,speed*10*1.02),"Core must keep up with the committed scale.");
                        Assert.That(demo.CurrentFrame.Scale,Is.EqualTo(speed));
                        if(speed==100) Assert.That(demo.CurrentFrame.L1,Is.Zero,"Warp preserves the existing coarse cap.");
                    }
                }
                File.WriteAllText(Path.Combine(Path.GetTempPath(),"worldsim72-editor-summary.json"),JsonUtility.ToJson(new Summary {cases=results.ToArray()},true));
            }
            finally
            {
                camera.targetTexture=previous;camera.rect=previousRect;Application.targetFrameRate=previousRate;target.Release();Object.DestroyImmediate(target);Object.DestroyImmediate(pixel);
            }
        }
    }
}
