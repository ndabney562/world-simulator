using System.Collections;
using System.IO;
using NUnit.Framework;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.TestTools;
using WorldSim.Rendering;

namespace WorldSim.Tests.EditMode
{
    public sealed class BridgePlayTests
    {
        [Test]
        public void LongStallDropsExcessWholeSteps()
        {
            double remainder=0;
            Assert.That(BridgeDemo.ConsumeHostElapsed(ref remainder,60),Is.EqualTo(100));
            Assert.That(remainder,Is.GreaterThanOrEqualTo(0).And.LessThan(0.1));
            for(int frame=0;frame<10;frame++)
                Assert.That(BridgeDemo.ConsumeHostElapsed(ref remainder,0),Is.Zero);
        }
        [Test]
        public void CatchUpPreservesOnlySubStepRemainder()
        {
            double remainder=0;
            Assert.That(BridgeDemo.ConsumeHostElapsed(ref remainder,60.025),Is.EqualTo(100));
            Assert.That(remainder,Is.EqualTo(0.025).Within(0.000000001));
            Assert.That(BridgeDemo.ConsumeHostElapsed(ref remainder,0.08),Is.EqualTo(1));
            Assert.That(remainder,Is.EqualTo(0.005).Within(0.000000001));
            Assert.That(BridgeDemo.ConsumeHostElapsed(ref remainder,0),Is.Zero);
        }
        [UnityTearDown]
        public IEnumerator LeavePlayModeAfterAnyFailure()
        {
            if (Application.isPlaying) yield return new ExitPlayMode();
        }
        [UnityTest]
        public IEnumerator BootstrapRendersAndAdvancesFromNativeSnapshots()
        {
            EditorSceneManager.OpenScene("Assets/WorldSim/Scenes/Bootstrap.unity",OpenSceneMode.Single);
            yield return new EnterPlayMode();
            var demo=Object.FindFirstObjectByType<BridgeDemo>();
            var camera=Camera.main;
            Assert.That(demo,Is.Not.Null);Assert.That(demo.Running,Is.True);
            Assert.That(camera,Is.Not.Null);
            var target=new RenderTexture(960,540,24);
            camera.targetTexture=target;
            ulong initial=demo.Tick;
            double deadline=Time.realtimeSinceStartupAsDouble+1.0;
            while(Time.realtimeSinceStartupAsDouble<deadline) yield return null;
            Assert.That(demo.Tick,Is.GreaterThan(initial));
            yield return null;
            var old=RenderTexture.active;RenderTexture.active=target;
            var image=new Texture2D(960,540,TextureFormat.RGB24,false);
            image.ReadPixels(new Rect(0,0,960,540),0,0);image.Apply();
            RenderTexture.active=old;
            // Ensure this is rendered cyan geometry rather than a blank camera image.
            int cyan=0;
            foreach(var pixel in image.GetPixels32())
                if(pixel.g>pixel.r*2 && pixel.b>pixel.r*2 && pixel.g>100) cyan++;
            Assert.That(cyan,Is.GreaterThan(100),"Native marker geometry must be visible.");
            string path=Path.Combine(Path.GetTempPath(),"worldsim-bridge-bootstrap.png");
            File.WriteAllBytes(path,image.EncodeToPNG());
            camera.targetTexture=null;
            Object.Destroy(image);Object.Destroy(target);
            yield return new ExitPlayMode();
        }
    }
}
