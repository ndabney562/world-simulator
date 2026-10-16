using System;
using UnityEngine;
using UnityEngine.Rendering;
using WorldSim.Bridge;

namespace WorldSim.Rendering
{
    /// <summary>TP-003 synchronous debug-marker demo; real entity rendering arrives in TP-011.</summary>
    public sealed class BridgeDemo : MonoBehaviour
    {
        [SerializeField, Range(0,100000)] int markerCount = 1000;
        NativeWorld world;
        Matrix4x4[] matrices;
        Mesh mesh;
        Material material;
        double pending;
        public ulong Tick { get; private set; }
        public bool Running => world != null;
        void OnEnable()
        {
            try
            {
                world=new NativeWorld(7,markerCount);
                matrices=new Matrix4x4[markerCount];
                var primitive=GameObject.CreatePrimitive(PrimitiveType.Cube);
                mesh=Instantiate(primitive.GetComponent<MeshFilter>().sharedMesh);
                DestroyImmediate(primitive);
                var shader=Shader.Find("Universal Render Pipeline/Unlit");
                if (shader == null) throw new InvalidOperationException("URP Unlit shader missing.");
                material=new Material(shader) { enableInstancing=true };
                material.SetColor("_BaseColor",new Color(0.15f,0.8f,0.9f));
                Tick=world.FillSnapshot(matrices);
            }
            catch (Exception error) { Debug.LogError($"Bridge demo stopped: {error.Message}",this); Release(); enabled=false; }
        }
        void Update()
        {
            try
            {
                pending += Time.unscaledDeltaTime;
                uint steps=(uint)Math.Min(100,Math.Floor(pending/0.1));
                if (steps>0) { world.Advance(steps); pending-=steps*0.1; }
                Tick=world.FillSnapshot(matrices);
                var parameters=new RenderParams(material) { worldBounds=new Bounds(new Vector3(100,0,1000),new Vector3(500,50,2500)), shadowCastingMode=ShadowCastingMode.Off, receiveShadows=false };
                for(int start=0; start<markerCount; start+=256)
                    Graphics.RenderMeshInstanced(parameters,mesh,0,matrices,Math.Min(256,markerCount-start),start);
            }
            catch (Exception error) { Debug.LogError($"Bridge demo stopped: {error.Message}",this); Release(); enabled=false; }
        }
        void OnDisable() { Release(); }
        void OnDestroy() { Release(); }
        void Release()
        {
            world?.Dispose(); world=null;
            if (mesh!=null) DestroyImmediate(mesh);
            if (material!=null) DestroyImmediate(material);
            mesh=null; material=null; pending=0;
        }
    }
}
