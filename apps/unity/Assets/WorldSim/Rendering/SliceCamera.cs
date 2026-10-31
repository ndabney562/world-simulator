using System;
using UnityEngine;

namespace WorldSim.Rendering
{
    /// <summary>Bounded real-block orbit/pan/pinch presentation and ADR0031 host-interest policy1.</summary>
    public sealed class SliceCamera
    {
        readonly Camera camera;
        readonly Bounds bounds;
        Vector2 lastPointer;
        bool orbiting,panning,defaultView=true,overview=true;
        float aspect;
        uint revision=1,submittedRevision;
        double lastSubmission=double.NegativeInfinity;
        public Vector3 Target { get; private set; }
        public float Distance { get; private set; }
        public float Yaw { get; private set; }=25;
        public float Pitch { get; private set; }=60;
        public float FitDistance { get; private set; }
        public bool IsDefaultView => defaultView;
        /// <summary>True when the last <see cref="Update"/> moved the view from player input.</summary>
        public bool UserMoved { get; private set; }
        Vector3 easeFrom,easeTo;
        float easeFromDistance,easeToDistance;
        double easeStart;
        public bool Easing { get; private set; }
        public const float MinimumDistance=4,MaximumDistance=1500;
        static bool Finite(float f)=>!float.IsNaN(f)&&!float.IsInfinity(f);
        public SliceCamera(Camera camera,Bounds bounds)
        {
            this.camera=camera!=null?camera:throw new ArgumentNullException(nameof(camera));this.bounds=bounds;Reset();
        }
        /// <summary>Fit all massing corners with margin and reset the overview latch.</summary>
        public void Reset()
        {
            Target=new Vector3(bounds.center.x,0.85f,bounds.center.z);Yaw=25;Pitch=60;defaultView=true;overview=true;
            Fit();revision++;Apply();
        }
        void Fit()
        {
            float actualAspect=camera.aspect;
            if(!Finite(actualAspect)||actualAspect<=0) throw new ArgumentException("Invalid camera aspect.");
            float tanY=Mathf.Tan(camera.fieldOfView*Mathf.Deg2Rad*0.5f)*0.9f,tanX=tanY*actualAspect;
            var inverse=Quaternion.Inverse(Quaternion.Euler(Pitch,Yaw,0));float distance=MinimumDistance;
            for(int i=0;i<8;i++)
            {
                var corner=bounds.center+new Vector3((i&1)==0?-bounds.extents.x:bounds.extents.x,(i&2)==0?-bounds.extents.y:bounds.extents.y,(i&4)==0?-bounds.extents.z:bounds.extents.z);
                Vector3 local=inverse*(corner-Target);
                distance=Mathf.Max(distance,Mathf.Max(Mathf.Abs(local.x)/tanX-local.z,Mathf.Abs(local.y)/tanY-local.z));
            }
            FitDistance=distance+1;Distance=Mathf.Clamp(FitDistance,MinimumDistance,MaximumDistance);aspect=actualAspect;
        }
        /// <summary>Resizing fits the default view; an intentionally moved target/zoom/orbit stays fixed.</summary>
        public void ResizeIfNeeded()
        {
            if(!Finite(camera.aspect)||camera.aspect<=0) throw new ArgumentException("Invalid camera aspect.");
            if(Mathf.Abs(aspect-camera.aspect)<=0.0001f) return;
            if(defaultView) Fit();else aspect=camera.aspect;revision++;Apply();
        }
        public void SetView(Vector3 target,float distance,float yaw,float pitch)
        {
            if(!Finite(target.x)||!Finite(target.y)||!Finite(target.z)||!Finite(distance)||!Finite(yaw)||!Finite(pitch)) throw new ArgumentException("Nonfinite camera pose.");
            Target=new Vector3(Mathf.Clamp(target.x,bounds.center.x-600,bounds.center.x+600),Mathf.Clamp(target.y,0.05f,30),Mathf.Clamp(target.z,bounds.center.z-600,bounds.center.z+600));
            Distance=Mathf.Clamp(distance,MinimumDistance,MaximumDistance);Yaw=Mathf.Repeat(yaw,360);Pitch=Mathf.Clamp(pitch,8,85);defaultView=false;revision++;Apply();
        }
        public void Orbit(Vector2 delta)
        {
            if(!Finite(delta.x)||!Finite(delta.y)) throw new ArgumentException("Nonfinite camera drag.");
            if(delta==Vector2.zero) return;SetView(Target,Distance,Yaw+delta.x*0.3f,Pitch-delta.y*0.3f);
        }
        public void Zoom(float factor)
        {
            if(!Finite(factor)||factor<=0) throw new ArgumentException("Invalid camera zoom.");
            if(factor==1) return;SetView(Target,Mathf.Clamp(Distance*factor,MinimumDistance,MaximumDistance),Yaw,Pitch);
        }
        public void Pan(Vector2 pixels)
        {
            if(!Finite(pixels.x)||!Finite(pixels.y)) throw new ArgumentException("Nonfinite camera pan.");
            if(pixels==Vector2.zero) return;
            Vector3 right=camera.transform.right;right.y=0;right.Normalize();Vector3 forward=camera.transform.forward;forward.y=0;forward.Normalize();
            float metres=2*Distance*Mathf.Tan(camera.fieldOfView*Mathf.Deg2Rad*0.5f)/Mathf.Max(1,camera.pixelHeight);
            SetView(Target-(right*pixels.x+forward*pixels.y)*metres,Distance,Yaw,Pitch);
        }
        /// <summary>Start a smooth move of the target (and zoom) from the current pose; yaw and pitch stay.</summary>
        public void EaseTo(Vector3 target,float distance,double now)
        {
            easeFrom=Target;easeTo=target;easeFromDistance=Distance;easeToDistance=distance;easeStart=now;Easing=true;
        }
        /// <summary>Advance an ease begun by <see cref="EaseTo"/>; the curve is supplied by the caller.</summary>
        public void StepEase(double now,Func<double,float> curve,double seconds)
        {
            if(!Easing) return;
            float s=curve(now-easeStart);
            SetView(Vector3.Lerp(easeFrom,easeTo,s),Mathf.Lerp(easeFromDistance,easeToDistance,s),Yaw,Pitch);
            // Tolerance: (start+seconds)-start can round just below seconds.
            if(now-easeStart>=seconds-1e-9) Easing=false;
        }
        /// <summary>Keep the view centred on a moving point (follow mode) without changing zoom or angle.</summary>
        public void Track(Vector3 point) {SetView(new Vector3(point.x,0.85f,point.z),Distance,Yaw,Pitch);}
        public void Update(bool blocked)
        {
            ResizeIfNeeded();UserMoved=false;
            if(blocked) {orbiting=false;panning=false;return;}
            uint before=revision;
            ReadInput();
            // Any player move takes over from an ease or a follow.
            if(revision!=before) {UserMoved=true;Easing=false;}
        }
        void ReadInput()
        {
            if(Input.touchCount==2)
            {
                var a=Input.GetTouch(0);var b=Input.GetTouch(1);
                float now=(a.position-b.position).magnitude,before=((a.position-a.deltaPosition)-(b.position-b.deltaPosition)).magnitude;
                Pan((a.deltaPosition+b.deltaPosition)*0.5f);if(before>1&&now>1) Zoom(before/now);orbiting=false;panning=false;
            }
            else if(Input.touchCount==1)
            {
                var touch=Input.GetTouch(0);if(touch.phase==TouchPhase.Moved) Orbit(touch.deltaPosition);
            }
            else if(Input.touchCount==0)
            {
                if(Input.GetMouseButtonDown(0)) {orbiting=true;lastPointer=Input.mousePosition;}
                if(Input.GetMouseButtonDown(1)||Input.GetMouseButtonDown(2)) {panning=true;orbiting=false;lastPointer=Input.mousePosition;}
                if(Input.GetMouseButtonUp(0)) orbiting=false;
                if(Input.GetMouseButtonUp(1)||Input.GetMouseButtonUp(2)) panning=false;
                Vector2 pointer=Input.mousePosition;
                if(orbiting) Orbit(pointer-lastPointer);if(panning) Pan(pointer-lastPointer);lastPointer=pointer;
                if(Input.mouseScrollDelta.y!=0) Zoom(Mathf.Pow(0.9f,Mathf.Clamp(Input.mouseScrollDelta.y,-20,20)));
            }
        }
        void Apply()
        {
            Quaternion rotation=Quaternion.Euler(Pitch,Yaw,0);camera.transform.SetPositionAndRotation(Target+rotation*new Vector3(0,0,-Distance),rotation);
            camera.nearClipPlane=0.05f;camera.farClipPlane=Mathf.Max(500,Distance+bounds.extents.magnitude+200);
        }
        bool GroundCorner(float x,float y,out Vector3 hit)
        {
            Ray ray=camera.ViewportPointToRay(new Vector3(x,y,0));hit=default;
            if(Mathf.Abs(ray.direction.y)<0.000001f) return false;
            float distance=-ray.origin.y/ray.direction.y;
            if(!Finite(distance)||distance<0||distance>1000) return false;
            hit=ray.GetPoint(distance);return Finite(hit.x)&&Finite(hit.y)&&Finite(hit.z);
        }
        /// <summary>Version1 overview latch and finite20–80m ground-footprint radius/fallback.</summary>
        public void Interest(out Vector3 target,out double radius)
        {
            target=Target;
            if(defaultView||Distance>=250) overview=true;else if(Distance<=225) overview=false;
            if(overview) {radius=0;return;}
            float maximum=0;
            for(int i=0;i<4;i++)
            {
                if(!GroundCorner((i&1)==0?0:1,(i&2)==0?0:1,out var hit)) {radius=Mathf.Clamp(Distance/4,20,80);return;}
                maximum=Mathf.Max(maximum,new Vector2(hit.x-Target.x,hit.z-Target.z).magnitude);
            }
            radius=Mathf.Clamp(maximum,20,80);
        }
        /// <summary>Coalesce unchanged pose/aspect and retain latest motion at a maximum10Hz.</summary>
        public bool TryInterest(double hostSeconds,out Vector3 target,out double radius)
        {
            target=default;radius=0;
            if(double.IsNaN(hostSeconds)||double.IsInfinity(hostSeconds)) throw new ArgumentException("Invalid camera host timestamp.");
            if(revision==submittedRevision||hostSeconds-lastSubmission<0.1-1e-12) return false;
            Interest(out target,out radius);submittedRevision=revision;lastSubmission=hostSeconds;return true;
        }
    }
}
