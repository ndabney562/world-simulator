using UnityEngine;

namespace WorldSim.Rendering
{
    /// <summary>Orbit/zoom camera around a target: one-finger drag or left mouse orbits; pinch or wheel zooms.</summary>
    public sealed class OrbitCamera
    {
        readonly Transform camera;
        public Vector3 Target;
        public float Distance = 220f, Yaw = 30f, Pitch = 35f;
        Vector2 lastPointer;
        bool dragging;
        public OrbitCamera(Transform camera, Vector3 target) { this.camera = camera; Target = target; Apply(); }
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
            Distance = Mathf.Clamp(Distance, 20f, 1500f);
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
            camera.position = Target + rotation * new Vector3(0, 0, -Distance);
            camera.rotation = rotation;
        }
    }
}
