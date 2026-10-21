using System.Collections;
using System.IO;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.Rendering.Universal;
using UnityEngine.SceneManagement;
using UnityEngine.TestTools;
using UnityEditor.SceneManagement;
using WorldSim.Rendering;
using Object=UnityEngine.Object;

namespace WorldSim.Tests.EditMode
{
    public sealed class SliceSceneTests
    {
        static bool FeaturedColour(Color32 p,int index)
        {
            switch(index)
            {
                case 0:return p.g>40&&p.b>40&&p.g>p.r*1.8f&&p.b>p.r*1.8f&&p.g>p.b*0.8f&&p.g<p.b*1.35f;
                case 1:return p.g>40&&p.g>p.r*1.8f&&p.g>p.b*1.8f;
                case 2:return p.r>40&&p.g>40&&p.r>p.b*1.8f&&p.g>p.b*1.8f&&p.r>p.g*0.8f&&p.r<p.g*1.35f;
                default:return p.r>40&&p.b>40&&p.r>p.g*1.8f&&p.b>p.g*1.8f&&p.r>p.b*0.7f&&p.r<p.b*1.5f;
            }
        }
        static Texture2D Capture(Camera camera,RenderTexture target)
        {
            RenderPipeline.SubmitRenderRequest(camera,new UniversalRenderPipeline.SingleCameraRequest {destination=target});
            var previous=RenderTexture.active;RenderTexture.active=target;
            var image=new Texture2D(target.width,target.height,TextureFormat.RGB24,false);image.ReadPixels(new Rect(0,0,target.width,target.height),0,0);image.Apply();RenderTexture.active=previous;return image;
        }
        [UnityTest]
        public IEnumerator RealBlockDrawsEveryFeaturedFootprintAndActualStreetResidentsFromNativeFrames()
        {
            yield return new EnterPlayMode();yield return ExerciseScene();yield return new ExitPlayMode();
        }
        static IEnumerator ExerciseScene()
        {
            var loading=EditorSceneManager.LoadSceneAsyncInPlayMode("Assets/WorldSim/Scenes/RealBlock.unity",new LoadSceneParameters(LoadSceneMode.Single));
            while(!loading.isDone) yield return null;
            var demo=Object.FindFirstObjectByType<SliceDemo>();Assert.That(demo,Is.Not.Null);
            Assert.That(demo.CurrentFrame.Population,Is.EqualTo(30));Assert.That(demo.Geometry.Buildings.Length,Is.EqualTo(58));
            var camera=demo.SceneCamera;RenderTexture previous=camera.targetTexture;RenderTexture target=null;Texture2D image=null;
            try
            {
                foreach(var dimensions in new[]{new Vector2Int(960,540),new Vector2Int(540,960)})
                {
                    target=new RenderTexture(dimensions.x,dimensions.y,24,RenderTextureFormat.ARGB32);target.Create();camera.targetTexture=target;
                    float scale=Mathf.Max(1,dimensions.x/1100f);camera.rect=new Rect(0,0,1,Mathf.Clamp((dimensions.y-266*scale)/dimensions.y,0.25f,1));
                    camera.aspect=dimensions.x/(dimensions.y*camera.rect.height);demo.ResetView();yield return null;yield return null;
                    image=Capture(camera,target);Color32[] pixels=image.GetPixels32();
                    for(int i=0;i<4;i++)
                    {
                        int count=0;foreach(var p in pixels) if(FeaturedColour(p,i)) count++;
                        Assert.That(count,Is.GreaterThan(16),$"Featured colour{i} must render at {dimensions}; pixels{count}.");
                    }
                    File.WriteAllBytes(Path.Combine(Path.GetTempPath(),$"worldsim72-overview-{dimensions.x}x{dimensions.y}.png"),image.EncodeToPNG());
                    Object.DestroyImmediate(image);image=null;camera.targetTexture=null;target.Release();Object.DestroyImmediate(target);target=null;
                }
                target=new RenderTexture(960,540,24,RenderTextureFormat.ARGB32);target.Create();camera.targetTexture=target;
                camera.rect=new Rect(0,0,1,1);camera.aspect=16f/9;
                demo.RequestStreetView();double timeout=Time.realtimeSinceStartupAsDouble+8;
                while(demo.WaitingForStreet&&Time.realtimeSinceStartupAsDouble<timeout) yield return null;
                Assert.That(demo.WaitingForStreet,Is.False,"Known configured commute must supply a nearby published local walker.");
                yield return null;yield return null;image=Capture(camera,target);
                int orange=0;foreach(var p in image.GetPixels32()) if(p.r>80&&p.r>p.g*1.5f&&p.g>p.b*1.5f) orange++;
                Assert.That(orange,Is.GreaterThan(20),"A real native resident must be visible from the street view.");
                Assert.That(demo.CurrentFrame.RenderedResidents,Is.GreaterThan(0));Assert.That(demo.CurrentFrame.Tick,Is.EqualTo(demo.CurrentFrame.Sky.Tick));
                File.WriteAllBytes(Path.Combine(Path.GetTempPath(),"worldsim72-street.png"),image.EncodeToPNG());
                demo.SetSpeed(0);yield return null;ulong tick=demo.CurrentFrame.Tick;
                yield return new WaitForSecondsRealtime(0.1f);Assert.That(demo.CurrentFrame.Tick,Is.EqualTo(tick));
                demo.RequestStreetView();yield return new WaitForSecondsRealtime(0.1f);
                Assert.That(demo.CurrentFrame.Scale,Is.Zero,"Camera shortcuts preserve an explicit pause.");Assert.That(demo.CurrentFrame.Tick,Is.EqualTo(tick));
                demo.SetSpeed(100);yield return new WaitForSecondsRealtime(0.1f);Assert.That(demo.CurrentFrame.Scale,Is.EqualTo(100));Assert.That(demo.CurrentFrame.Tick,Is.GreaterThan(tick));
                Assert.That(demo.CalendarText,Does.Contain("UTC").And.Contain("NY").And.Contain("100×"));
            }
            finally
            {
                camera.targetTexture=previous;if(image!=null) Object.DestroyImmediate(image);
                if(target!=null) {target.Release();Object.DestroyImmediate(target);}
            }
        }
    }
}
