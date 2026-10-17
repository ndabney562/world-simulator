using System.IO;
using UnityEditor;
using UnityEditor.Build;
using UnityEditor.Build.Reporting;
using UnityEditor.iOS.Xcode;
using UnityEngine;
using UnityEngine.Rendering;

namespace WorldSim.EditorTools
{
    /// <summary>TP-011: one menu item that prepares and builds the iPhone performance-spike Xcode project.</summary>
    public static class IosBuild
    {
        const string Scene = "Assets/WorldSim/Scenes/Bootstrap.unity";
        const string Plugin = "Assets/Plugins/iOS/libws_ffi.a";
        const string OutputFolder = "Builds/iOS";

        [MenuItem("WorldSim/Build iOS performance spike")]
        public static void Build()
        {
            if (!File.Exists(Plugin))
            {
                EditorUtility.DisplayDialog("Native plugin missing",
                    "Run scripts/build-native-ios.sh first. It builds libws_ffi.a for aarch64-apple-ios into Assets/Plugins/iOS.", "OK");
                return;
            }
            EditorUserBuildSettings.SwitchActiveBuildTarget(NamedBuildTarget.iOS, BuildTarget.iOS);
            PlayerSettings.SetScriptingBackend(NamedBuildTarget.iOS, ScriptingImplementation.IL2CPP);
            PlayerSettings.SetArchitecture(NamedBuildTarget.iOS, 1); // ARM64
            PlayerSettings.SetGraphicsAPIs(BuildTarget.iOS, new[] { GraphicsDeviceType.Metal });
            PlayerSettings.SetApplicationIdentifier(NamedBuildTarget.iOS, "org.worldsimulator.tp011");
            PlayerSettings.iOS.targetOSVersionString = "17.0";
            AlwaysInclude("Universal Render Pipeline/Unlit");
            var options = new BuildPlayerOptions
            {
                scenes = new[] { Scene },
                target = BuildTarget.iOS,
                locationPathName = OutputFolder,
                options = BuildOptions.None,
            };
            BuildReport report = BuildPipeline.BuildPlayer(options);
            if (report.summary.result != BuildResult.Succeeded)
                throw new BuildFailedException($"iOS build {report.summary.result}: {report.summary.totalErrors} errors");
            Debug.Log($"Xcode project written to {OutputFolder}. Open Unity-iPhone.xcodeproj, pick your personal team, and run on the device.");
        }

        /// <summary>Shader.Find in a player only sees shaders that are in the build; URP Unlit is not referenced by a scene asset.</summary>
        static void AlwaysInclude(string shaderName)
        {
            var shader = Shader.Find(shaderName);
            if (shader == null) throw new BuildFailedException($"Shader {shaderName} not found.");
            var settings = AssetDatabase.LoadAssetAtPath<Object>("ProjectSettings/GraphicsSettings.asset");
            var serialized = new SerializedObject(settings);
            var list = serialized.FindProperty("m_AlwaysIncludedShaders");
            for (int i = 0; i < list.arraySize; i++)
                if (list.GetArrayElementAtIndex(i).objectReferenceValue == shader) return;
            list.InsertArrayElementAtIndex(list.arraySize);
            list.GetArrayElementAtIndex(list.arraySize - 1).objectReferenceValue = shader;
            serialized.ApplyModifiedPropertiesWithoutUndo();
        }

        /// <summary>Lets the device run logs be copied out through the Files app and Finder file sharing.</summary>
        [UnityEditor.Callbacks.PostProcessBuild]
        public static void AllowLogSharing(BuildTarget target, string path)
        {
            if (target != BuildTarget.iOS) return;
            string plistPath = Path.Combine(path, "Info.plist");
            var plist = new PlistDocument();
            plist.ReadFromFile(plistPath);
            plist.root.SetBoolean("UIFileSharingEnabled", true);
            plist.root.SetBoolean("LSSupportsOpeningDocumentsInPlace", true);
            plist.WriteToFile(plistPath);
        }
    }
}
