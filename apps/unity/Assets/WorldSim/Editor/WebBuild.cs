using System;
using System.IO;
using UnityEditor;
using UnityEditor.Build;
using UnityEditor.Build.Reporting;
using UnityEngine;

namespace WorldSim.EditorTools
{
    /// <summary>Reproducible local desktop-browser compatibility export; no publication.</summary>
    public static class WebBuild
    {
        public static void Build()
        {
            if(Application.unityVersion!="6000.3.26f1") throw new BuildFailedException("Use Unity 6000.3.26f1 with WebGL Support.");
            if(!File.Exists("Assets/Plugins/WebGL/libws_ffi.a")) throw new BuildFailedException("Run scripts/build-native-webgl.sh first.");
            // Back up exact serialized bytes, including unrelated target settings.
            string player="ProjectSettings/ProjectSettings.asset",graphics="ProjectSettings/GraphicsSettings.asset";
            byte[] oldPlayer=File.ReadAllBytes(player),oldGraphics=File.ReadAllBytes(graphics);
            try
            {
                PlayerSettings.WebGL.compressionFormat=WebGLCompressionFormat.Gzip;
                PlayerSettings.WebGL.decompressionFallback=true; // Plain static hosts need no encoding headers.
                PlayerSettings.WebGL.dataCaching=false;
                PlayerSettings.WebGL.template="PROJECT:Desktop";
                PlayerSettings.WebGL.initialMemorySize=128;
                PlayerSettings.runInBackground=false;
                PlayerSettings.productName="World Simulator";
                var settings=new SerializedObject(AssetDatabase.LoadAssetAtPath<UnityEngine.Object>(graphics));
                settings.FindProperty("m_InstancingStripping").intValue=2;
                var list=settings.FindProperty("m_AlwaysIncludedShaders");
                foreach(string name in new[]{"Universal Render Pipeline/Lit","Universal Render Pipeline/Unlit"})
                {
                    Shader shader=Shader.Find(name);
                    if(shader==null) throw new BuildFailedException("Missing shader: "+name);
                    bool included=false;
                    for(int i=0;i<list.arraySize;i++) included|=list.GetArrayElementAtIndex(i).objectReferenceValue==shader;
                    if(!included) {list.InsertArrayElementAtIndex(list.arraySize);list.GetArrayElementAtIndex(list.arraySize-1).objectReferenceValue=shader;}
                }
                settings.ApplyModifiedPropertiesWithoutUndo();
                var report=BuildPipeline.BuildPlayer(new BuildPlayerOptions {
                    scenes=new[]{"Assets/WorldSim/Scenes/RealBlock.unity"},target=BuildTarget.WebGL,
                    locationPathName="Builds/DesktopWeb",options=BuildOptions.None
                });
                if(report.summary.result!=BuildResult.Succeeded) throw new BuildFailedException("Desktop browser export failed: "+report.summary.totalErrors+" errors");
                Debug.Log("WORLD_SIM_DESKTOP_WEB_BUILD_OK");
            }
            finally
            {
                File.WriteAllBytes(player,oldPlayer);File.WriteAllBytes(graphics,oldGraphics);
                AssetDatabase.Refresh();
            }
        }
    }
}
