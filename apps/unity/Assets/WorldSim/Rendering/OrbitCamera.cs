using UnityEngine;

namespace WorldSim.Rendering
{
    /// <summary>Orbit/zoom camera around a target: one-finger drag or left mouse orbits; pinch or wheel zooms.</summary>
    public sealed class OrbitCamera
    {
        readonly Camera camera;
        float sceneRadius, maxDistance = 1500f, fitAspect;
        public Vector3 Target;
        public float Distance = 220f, Yaw = 30f, Pitch = 35f;
        Vector2 lastPointer;
        bool dragging;
        public OrbitCamera(Camera camera, Vector3 target) { this.camera = camera; Target = target; Apply(); }
        /// <summary>Distance at which the last <see cref="FrameBounds"/> fit the bounds; 0 before the first fit.</summary>
        public float FitDistance { get; private set; }
        /// <summary>True once fitted and the camera's aspect is still the one it was fitted for.</summary>
        public bool FitsAspect => FitDistance > 0 && Mathf.Abs(fitAspect - camera.aspect) <= 0.001f;
        /// <summary>Called every frame; <paramref name="blocked"/> is true while the pointer is over HUD controls.</summary>
        public void Update(bool blocked)
        {
            if (Input.touchCount == 2)
            {
                var a = Input.GetTouch(0); var b = Input.GetTouch(1);
                float now = (a.position - b.position).magnitude;
                float before = ((a.position - a.deltaPosition) - (b.position - b.deltaPosition)).magnitude;
                if (before > 1f) Distance *= before / Mathf.Max(now, 1f);
                dragging = false;
            }
            else if (Input.touchCount == 1 && !blocked)
            {
                var t = Input.GetTouch(0);
                if (t.phase == TouchPhase.Moved) Rotate(t.deltaPosition);
            }
            else if (Input.touchCount == 0)
            {
                if (Input.GetMouseButtonDown(0)) { dragging = !blocked; lastPointer = Input.mousePosition; }
                if (Input.GetMouseButtonUp(0)) dragging = false;
                if (dragging) { Vector2 p = Input.mousePosition; Rotate(p - lastPointer); lastPointer = p; }
                float wheel = Input.mouseScrollDelta.y;
                if (wheel != 0 && !blocked) Distance *= Mathf.Pow(0.9f, wheel);
            }
            Distance = Mathf.Clamp(Distance, 20f, maxDistance);
            Apply();
        }
        /// <summary>
        /// Fit every bounds corner inside the perspective view, including portrait aspect ratios. With
        /// <paramref name="keepZoom"/> (screen rotation, window resize) the user's zoom relative to the fit is kept.
        /// </summary>
        public void FrameBounds(Bounds bounds, bool keepZoom = false)
        {
            float zoom = keepZoom && FitDistance > 0 ? Distance / FitDistance : 1f;
            Target = bounds.center;
            sceneRadius = bounds.extents.magnitude;
            var inverse = Quaternion.Inverse(Quaternion.Euler(Pitch, Yaw, 0));
            float tanY = Mathf.Tan(camera.fieldOfView * Mathf.Deg2Rad * 0.5f) * 0.9f;
            float tanX = tanY * Mathf.Max(camera.aspect, 0.01f);
            float distance = 20f;
            for (int corner = 0; corner < 8; corner++)
            {
                var offset = new Vector3((corner & 1) == 0 ? -bounds.extents.x : bounds.extents.x,
                    (corner & 2) == 0 ? -bounds.extents.y : bounds.extents.y,
                    (corner & 4) == 0 ? -bounds.extents.z : bounds.extents.z);
                var local = inverse * offset;
                distance = Mathf.Max(distance, Mathf.Max(Mathf.Abs(local.x) / tanX - local.z,
                    Mathf.Abs(local.y) / tanY - local.z));
            }
            FitDistance = distance + 1f;
            fitAspect = camera.aspect;
            maxDistance = Mathf.Max(1500f, FitDistance * 2f);
            Distance = Mathf.Clamp(FitDistance * zoom, 20f, maxDistance);
            Apply();
        }

        void Rotate(Vector2 delta)
        {
            Yaw += delta.x * 0.3f;
            Pitch = Mathf.Clamp(Pitch - delta.y * 0.3f, 8f, 85f);
        }
        void Apply()
        {
            var rotation = Quaternion.Euler(Pitch, Yaw, 0);
            camera.transform.position = Target + rotation * new Vector3(0, 0, -Distance);
            camera.transform.rotation = rotation;
            // The toy world spans about 1 km: overview/zoom-out distances exceed the scene's old 1 km far plane.
            camera.farClipPlane = Mathf.Max(1000f, Distance + sceneRadius + 100f);
        }
    }
}
