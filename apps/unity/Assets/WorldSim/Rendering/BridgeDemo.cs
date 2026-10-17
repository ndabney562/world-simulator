using System;
using System.Globalization;
using System.IO;
using System.Text;
using UnityEngine;
using UnityEngine.Profiling;
using UnityEngine.Rendering;
using WorldSim.Bridge;

namespace WorldSim.Rendering
{
    /// <summary>
    /// TP-011 toy-world host: starts a worker-driven native world, draws buildings as boxes and residents as
    /// capsules with instancing, and shows the debug HUD. The scale buttons rebuild the world at 50, 500 or 5,000
    /// residents. The class keeps its TP-003 name so the Bootstrap scene reference stays valid.
    /// </summary>
    public sealed class BridgeDemo : MonoBehaviour
    {
        static readonly int[] Scales = { 50, 500, 5000 };
        const int Chunk = 256;
        [SerializeField] int startScaleIndex = 0;
        SimHost host;
        RenderFrame frame;
        Mesh boxMesh, capsuleMesh;
        Material buildingMaterial, residentMaterial;
        OrbitCamera orbit;
        Rect hudRect;
        int scaleIndex;
        bool framed;
        // Metrics over the current half-second window and the last published window.
        float windowStart, windowFrameSum, windowFrameMax, windowFillSum;
        int windowFrames;
        double fps, frameMean, frameMax, fillMean, advanceMean, advanceMax;
        StreamWriter log;
        float runStart, nextLog;
        string logPath = "";
        public ulong Tick { get; private set; }
        public bool Running => host != null;

        void OnEnable()
        {
            try
            {
                var primitive = GameObject.CreatePrimitive(PrimitiveType.Cube);
                boxMesh = Instantiate(primitive.GetComponent<MeshFilter>().sharedMesh);
                DestroyImmediate(primitive);
                primitive = GameObject.CreatePrimitive(PrimitiveType.Capsule);
                capsuleMesh = Instantiate(primitive.GetComponent<MeshFilter>().sharedMesh);
                DestroyImmediate(primitive);
                var shader = Shader.Find("Universal Render Pipeline/Unlit");
                if (shader == null) throw new InvalidOperationException("URP Unlit shader missing.");
                buildingMaterial = new Material(shader) { enableInstancing = true };
                buildingMaterial.SetColor("_BaseColor", new Color(0.15f, 0.8f, 0.9f));
                residentMaterial = new Material(shader) { enableInstancing = true };
                residentMaterial.SetColor("_BaseColor", new Color(1f, 0.55f, 0.1f));
                Application.targetFrameRate = 60;
                orbit = new OrbitCamera(Camera.main.transform, Vector3.zero);
                StartWorld(Mathf.Clamp(startScaleIndex, 0, Scales.Length - 1));
            }
            catch (Exception error) { Debug.LogError($"Toy world stopped: {error.Message}", this); Release(); enabled = false; }
        }

        /// <summary>Replace the world with one of the three scale steps and begin a fresh metrics log.</summary>
        void StartWorld(int index)
        {
            StopWorld();
            scaleIndex = index;
            int residents = Scales[index];
            host = new SimHost(7, residents / NativeWorld.BuildingCount);
            frame = new RenderFrame(NativeWorld.BuildingCount, residents);
            framed = false;
            host.Fill(frame); Tick = frame.Tick;
            OpenLog(residents);
            windowStart = Time.realtimeSinceStartup; windowFrames = 0; windowFrameSum = windowFrameMax = windowFillSum = 0;
        }

        void OpenLog(int residents)
        {
            log?.Dispose(); log = null;
            try
            {
                logPath = Path.Combine(Application.persistentDataPath, $"tp011-{residents}-{DateTime.UtcNow:yyyyMMdd-HHmmss}.csv");
                log = new StreamWriter(logPath, false, new UTF8Encoding(false));
                log.WriteLine("t_s,residents,fps,frame_ms_mean,frame_ms_max,fill_ms,advance_ms_mean,advance_ms_max,instances,managed_mb,native_footprint_mb,thermal");
            }
            catch (Exception error) { Debug.LogWarning($"Run log unavailable: {error.Message}"); log = null; logPath = "(none)"; }
            runStart = Time.realtimeSinceStartup; nextLog = runStart + 1f;
        }

        void Update()
        {
            if (host == null) return;
            try
            {
                if (host.Failure != null) throw host.Failure;
                double fill = host.Fill(frame);
                Tick = frame.Tick;
                if (!framed) Frame();
                orbit.Update(hudRect.Contains(HudPointer()));
                Draw();
                Sample(Time.unscaledDeltaTime * 1000f, (float)fill);
            }
            catch (Exception error) { Debug.LogError($"Toy world stopped: {error.Message}", this); Release(); enabled = false; }
        }

        static Vector2 HudPointer()
        {
            Vector2 p = Input.touchCount > 0 ? Input.GetTouch(0).position : (Vector2)Input.mousePosition;
            return new Vector2(p.x, Screen.height - p.y) / HudScale();
        }
        static float HudScale() => Mathf.Max(1f, Screen.dpi > 0 ? Screen.dpi / 160f : 1f);

        void Frame()
        {
            var min = new Vector3(float.MaxValue, 0, float.MaxValue); var max = new Vector3(float.MinValue, 0, float.MinValue);
            for (int i = 0; i < frame.BuildingCount; i++)
            {
                Vector3 p = frame.Buildings[i].GetColumn(3);
                min = Vector3.Min(min, p); max = Vector3.Max(max, p);
            }
            orbit.Target = new Vector3((min.x + max.x) * 0.5f, 0, (min.z + max.z) * 0.5f);
            orbit.Distance = Mathf.Max(120f, (max - min).magnitude * 1.4f);
            framed = true;
        }

        void Draw()
        {
            var bounds = new Bounds(orbit.Target, new Vector3(4000, 400, 4000));
            Submit(buildingMaterial, boxMesh, frame.Buildings, frame.BuildingCount, bounds);
            Submit(residentMaterial, capsuleMesh, frame.Residents, frame.ResidentCount, bounds);
        }

        static void Submit(Material material, Mesh mesh, Matrix4x4[] matrices, int count, Bounds bounds)
        {
            var parameters = new RenderParams(material) { worldBounds = bounds, shadowCastingMode = ShadowCastingMode.Off, receiveShadows = false };
            for (int start = 0; start < count; start += Chunk)
                Graphics.RenderMeshInstanced(parameters, mesh, 0, matrices, Math.Min(Chunk, count - start), start);
        }

        void Sample(float frameMs, float fillMs)
        {
            windowFrames++; windowFrameSum += frameMs; windowFillSum += fillMs;
            windowFrameMax = Mathf.Max(windowFrameMax, frameMs);
            float now = Time.realtimeSinceStartup;
            if (now - windowStart >= 0.5f)
            {
                frameMean = windowFrameSum / windowFrames; frameMax = windowFrameMax; fillMean = windowFillSum / windowFrames;
                fps = 1000.0 / Math.Max(frameMean, 0.001);
                host.TakeAdvanceStats(out advanceMean, out advanceMax);
                windowStart = now; windowFrames = 0; windowFrameSum = windowFrameMax = windowFillSum = 0;
            }
            if (log != null && now >= nextLog)
            {
                nextLog += 1f;
                var c = CultureInfo.InvariantCulture;
                log.WriteLine(string.Join(",", (now - runStart).ToString("F1", c), Scales[scaleIndex], fps.ToString("F1", c),
                    frameMean.ToString("F2", c), frameMax.ToString("F2", c), fillMean.ToString("F3", c),
                    advanceMean.ToString("F3", c), advanceMax.ToString("F3", c), host.InstanceCount,
                    (GC.GetTotalMemory(false) / 1048576.0).ToString("F1", c), (PlatformStats.FootprintBytes / 1048576.0).ToString("F1", c),
                    PlatformStats.ThermalState));
                log.Flush();
            }
        }

        void OnGUI()
        {
            if (host == null) return;
            float scale = HudScale();
            GUI.matrix = Matrix4x4.Scale(new Vector3(scale, scale, 1));
            hudRect = new Rect(8, 8, 330, 224);
            GUI.Box(hudRect, GUIContent.none);
            GUILayout.BeginArea(new Rect(14, 12, 318, 216));
            GUILayout.Label($"{fps:F0} fps   frame {frameMean:F1} ms (max {frameMax:F1})");
            GUILayout.Label($"core step {advanceMean:F2} ms (max {advanceMax:F2})   fill {fillMean:F2} ms");
            GUILayout.Label($"{host.ResidentCount} residents, {host.InstanceCount} instances, tick {Tick}");
            GUILayout.Label($"mem managed {GC.GetTotalMemory(false) / 1048576.0:F0} MB, unity {Profiler.GetTotalAllocatedMemoryLong() / 1048576.0:F0} MB, process {PlatformStats.FootprintBytes / 1048576.0:F0} MB");
            GUILayout.Label($"thermal {PlatformStats.ThermalName(PlatformStats.ThermalState)}   log {Path.GetFileName(logPath)}");
            GUILayout.BeginHorizontal();
            if (GUILayout.Button("Pause")) host.SetScale(0);
            if (GUILayout.Button("x1")) host.SetScale(1);
            if (GUILayout.Button("x10")) host.SetScale(10);
            GUILayout.EndHorizontal();
            GUILayout.BeginHorizontal();
            for (int i = 0; i < Scales.Length; i++)
                if (GUILayout.Toggle(scaleIndex == i, Scales[i].ToString(), GUI.skin.button) && scaleIndex != i)
                { StartWorld(i); break; }
            GUILayout.EndHorizontal();
            GUILayout.EndArea();
        }

        /// <summary>Take at most 100 ticks; discard excess whole steps and retain sub-step time.</summary>
        public static uint ConsumeHostElapsed(ref double remainder, double elapsed, uint scale = 1)
        {
            if (double.IsNaN(elapsed) || double.IsInfinity(elapsed) || elapsed < 0)
                throw new ArgumentOutOfRangeException(nameof(elapsed));
            if (scale != 0 && scale != 1 && scale != 10 && scale != 100)
                throw new ArgumentOutOfRangeException(nameof(scale));
            double total = remainder + elapsed * scale;
            double whole = Math.Floor(total / 0.1);
            remainder = Math.Max(0, total - whole * 0.1);
            return (uint)Math.Min(100, whole);
        }

        void OnDisable() { Release(); }
        void OnDestroy() { Release(); }
        void StopWorld()
        {
            host?.Dispose(); host = null;
        }
        void Release()
        {
            StopWorld();
            log?.Dispose(); log = null;
            if (boxMesh != null) DestroyImmediate(boxMesh);
            if (capsuleMesh != null) DestroyImmediate(capsuleMesh);
            if (buildingMaterial != null) DestroyImmediate(buildingMaterial);
            if (residentMaterial != null) DestroyImmediate(residentMaterial);
            boxMesh = capsuleMesh = null; buildingMaterial = residentMaterial = null;
        }
    }
}
