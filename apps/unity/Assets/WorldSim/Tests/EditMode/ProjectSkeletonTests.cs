using System;
using System.IO;
using System.Linq;
using NUnit.Framework;
using UnityEditor;
using UnityEditor.Build;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.Rendering.Universal;

namespace WorldSim.Tests.EditMode
{
    /// <summary>
    /// Checks the TP-002 project skeleton: the assembly layout, the iOS player
    /// settings, the URP asset, and the asset serialization settings. These are
    /// the settings the issue requires, so a change to any of them shows up here
    /// rather than on a device build.
    /// </summary>
    public sealed class ProjectSkeletonTests
    {
        // ADR 0004: the current and previous major iOS versions. iOS 27 is
        // current as of 2026-10, so the minimum is 26.0. Raise it when a new
        // major iOS version ships.
        const string MinimumIosVersion = "26.0";

        const string PlaceholderBundleIdentifier = "org.worldsimulator.placeholder";
        const string RealBlockScenePath = "Assets/WorldSim/Scenes/RealBlock.unity";
        const string ProvidersFolder = "Assets/WorldSim/Providers";

        // PlayerSettings architecture value for arm64 on iOS (0 = armv7, 1 = arm64, 2 = universal).
        const int IosArm64 = 1;

        [Serializable]
        sealed class AssemblyDefinition
        {
            public string name;
            public string[] references;
        }

        static AssemblyDefinition ReadAssemblyDefinition(string path)
        {
            Assert.That(File.Exists(path), Is.True, $"Missing assembly definition {path}");
            return JsonUtility.FromJson<AssemblyDefinition>(File.ReadAllText(path));
        }

        [Test]
        public void WorldSimAssembliesAreCompiled()
        {
            var names = AppDomain.CurrentDomain.GetAssemblies().Select(a => a.GetName().Name).ToArray();
            Assert.That(names, Does.Contain("WorldSim.Bridge"));
            Assert.That(names, Does.Contain("WorldSim.Rendering"));
            Assert.That(names, Does.Contain("WorldSim.Tests.EditMode"));
        }

        [Test]
        public void RenderingReferencesBridgeAndBridgeReferencesNoWorldSimAssembly()
        {
            var bridge = ReadAssemblyDefinition("Assets/WorldSim/Bridge/WorldSim.Bridge.asmdef");
            var rendering = ReadAssemblyDefinition("Assets/WorldSim/Rendering/WorldSim.Rendering.asmdef");

            Assert.That(bridge.name, Is.EqualTo("WorldSim.Bridge"));
            Assert.That(rendering.name, Is.EqualTo("WorldSim.Rendering"));
            Assert.That(rendering.references, Is.EquivalentTo(new[] { "WorldSim.Bridge" }));
            Assert.That(bridge.references ?? Array.Empty<string>(), Has.None.StartsWith("WorldSim."));
        }

        [Test]
        public void ProvidersFolderContainsNoCode()
        {
            Assert.That(Directory.Exists(ProvidersFolder), Is.True);
            var code = Directory.GetFiles(ProvidersFolder, "*.cs", SearchOption.AllDirectories);
            Assert.That(code, Is.Empty, "Provider code arrives with TP-012, not before.");
        }

        [Test]
        public void IosPlayerSettingsMatchTheIssue()
        {
            Assert.That(PlayerSettings.GetScriptingBackend(NamedBuildTarget.iOS), Is.EqualTo(ScriptingImplementation.IL2CPP));
            Assert.That(PlayerSettings.GetArchitecture(NamedBuildTarget.iOS), Is.EqualTo(IosArm64));
            // iOS has only Metal in the pinned Unity Editor. The automatic-selection
            // flag is not the contract: verify the actual API list below instead.
            Assert.That(PlayerSettings.GetGraphicsAPIs(BuildTarget.iOS), Is.EqualTo(new[] { GraphicsDeviceType.Metal }));
            Assert.That(PlayerSettings.iOS.targetOSVersionString, Is.EqualTo(MinimumIosVersion));
            Assert.That(PlayerSettings.iOS.targetDevice, Is.EqualTo(iOSTargetDevice.iPhoneOnly));
            Assert.That(PlayerSettings.iOS.sdkVersion, Is.EqualTo(iOSSdkVersion.DeviceSDK));
            Assert.That(PlayerSettings.GetApplicationIdentifier(NamedBuildTarget.iOS), Is.EqualTo(PlaceholderBundleIdentifier));
            Assert.That(PlayerSettings.colorSpace, Is.EqualTo(ColorSpace.Linear));
        }

        [Test]
        public void UrpIsTheActiveRenderPipeline()
        {
            Assert.That(GraphicsSettings.defaultRenderPipeline, Is.InstanceOf<UniversalRenderPipelineAsset>());
            Assert.That(GraphicsSettings.currentRenderPipeline, Is.SameAs(GraphicsSettings.defaultRenderPipeline),
                "A quality level overrides the project's URP asset.");
        }

        [Test]
        public void UrpAssetUsesConservativeMobileDefaults()
        {
            var urp = (UniversalRenderPipelineAsset)GraphicsSettings.defaultRenderPipeline;
            Assert.That(urp, Is.Not.Null);
            Assert.That(urp.supportsHDR, Is.False, "HDR is off until TP-011 measures its cost.");
            Assert.That(urp.msaaSampleCount, Is.EqualTo(1), "MSAA is off until TP-011 measures its cost.");
            Assert.That(urp.renderScale, Is.EqualTo(1f));
            Assert.That(urp.shadowCascadeCount, Is.EqualTo(1));
            Assert.That(urp.supportsCameraDepthTexture, Is.False);
            Assert.That(urp.supportsCameraOpaqueTexture, Is.False);
            Assert.That(urp.useSRPBatcher, Is.True);
        }

        [Test]
        public void AssetsAreTextSerializedWithVisibleMetaFiles()
        {
            Assert.That(EditorSettings.serializationMode, Is.EqualTo(SerializationMode.ForceText));
            Assert.That(VersionControlSettings.mode, Is.EqualTo("Visible Meta Files"));
        }

        [Test]
        public void RealBlockIsTheOnlyBuildScene()
        {
            var scenes = EditorBuildSettings.scenes;
            Assert.That(scenes.Select(s => s.path), Is.EqualTo(new[] { RealBlockScenePath }));
            Assert.That(scenes[0].enabled, Is.True);
        }
    }
}
