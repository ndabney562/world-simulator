using System;
using UnityEngine;
using WorldSim.Bridge;

namespace WorldSim.Rendering
{
    public struct SliceHit
    {
        public ulong Id,Bin;
        public bool Resident;
        public float Distance;
    }
    /// <summary>Tap state is independent of camera drag; cancellation stays latched.</summary>
    public sealed class SliceTapGesture
    {
        bool active,cancelled;
        int pointer;
        Vector2 start;
        double began;
        public bool UiCaptured { get; private set; }
        public void Begin(int pointer,Vector2 position,double seconds,bool overUi)
        {
            this.pointer=pointer;start=position;began=seconds;active=true;cancelled=overUi;UiCaptured=overUi;
        }
        public void Move(int pointer,Vector2 position,bool overUi)
        {
            if(!active||pointer!=this.pointer) return;
            if(!Finite(position.x)||!Finite(position.y)||(position-start).sqrMagnitude>64||overUi) cancelled=true;
            if(overUi) UiCaptured=true;
        }
        public bool End(int pointer,Vector2 position,double seconds,bool overUi)
        {
            if(!active||pointer!=this.pointer) return false;
            Move(pointer,position,overUi);bool tap=!cancelled&&Finite(start.x)&&Finite(start.y)&&!double.IsNaN(seconds)&&!double.IsInfinity(seconds)&&seconds>=began&&seconds-began<=0.35;
            active=false;UiCaptured=false;return tap;
        }
        public void Cancel() {active=false;cancelled=true;UiCaptured=false;}
        static bool Finite(float n)=>!float.IsNaN(n)&&!float.IsInfinity(n);
    }
    /// <summary>Tap-only ray/triangle picking uses exactly the cached rendered meshes and instance transforms.</summary>
    public sealed class SlicePicker
    {
        readonly SliceGeometry geometry;
        readonly Vector3[] capsule;
        readonly int[] triangles;
        public SlicePicker(SliceGeometry geometry,Mesh capsule)
        {
            this.geometry=geometry??throw new ArgumentNullException(nameof(geometry));
            if(capsule==null) throw new ArgumentNullException(nameof(capsule));this.capsule=capsule.vertices;triangles=capsule.triangles;
        }
        static bool Finite(float n)=>!float.IsNaN(n)&&!float.IsInfinity(n);
        static bool Finite(Vector3 p)=>Finite(p.x)&&Finite(p.y)&&Finite(p.z);
        static bool Triangle(Vector3 origin,Vector3 direction,Vector3 a,Vector3 b,Vector3 c,out float distance)
        {
            distance=0;Vector3 edge1=b-a,edge2=c-a,p=Vector3.Cross(direction,edge2);float determinant=Vector3.Dot(edge1,p);
            if(Mathf.Abs(determinant)<1e-8f) return false;
            float inverse=1/determinant;Vector3 relative=origin-a;float u=Vector3.Dot(relative,p)*inverse;if(u<0||u>1) return false;
            Vector3 q=Vector3.Cross(relative,edge1);float v=Vector3.Dot(direction,q)*inverse;if(v<0||u+v>1) return false;
            distance=Vector3.Dot(edge2,q)*inverse;return Finite(distance)&&distance>0&&distance<=10000;
        }
        static void Consider(ref bool found,ref SliceHit best,SliceHit hit)
        {
            int priority=hit.Resident?0:hit.Id!=0?1:2,bestPriority=best.Resident?0:best.Id!=0?1:2;
            ulong key=hit.Id!=0?hit.Id:hit.Bin,bestKey=best.Id!=0?best.Id:best.Bin;
            if(!found||hit.Distance<best.Distance||(hit.Distance==best.Distance&&(priority<bestPriority||(priority==bestPriority&&key<bestKey)))) {found=true;best=hit;}
        }
        public bool TryPick(Ray ray,SliceFrame frame,out SliceHit hit)
        {
            hit=default;if(frame==null||!Finite(ray.origin)||!Finite(ray.direction)||ray.direction.sqrMagnitude<1e-12f||frame.RenderedResidents<0||frame.RenderedResidents>30) return false;
            ray.direction=ray.direction.normalized;bool found=false;
            foreach(var building in geometry.Buildings)
                for(int i=building.FirstIndex;i<building.FirstIndex+building.IndexCount;i+=3)
                    if(Triangle(ray.origin,ray.direction,geometry.Vertices[geometry.Indices[i]],geometry.Vertices[geometry.Indices[i+1]],geometry.Vertices[geometry.Indices[i+2]],out float distance))
                        Consider(ref found,ref hit,new SliceHit {Id=building.Id,Bin=building.Bin,Distance=distance});
            for(int instance=0;instance<frame.RenderedResidents;instance++)
            {
                Matrix4x4 inverse=frame.ResidentMatrices[instance].inverse;
                Vector3 localOrigin=inverse.MultiplyPoint3x4(ray.origin),localDirection=inverse.MultiplyVector(ray.direction);
                // Preserve the transformed direction length: t is still original world-ray distance.
                for(int i=0;i<triangles.Length;i+=3)
                    if(Triangle(localOrigin,localDirection,capsule[triangles[i]],capsule[triangles[i+1]],capsule[triangles[i+2]],out float distance))
                        Consider(ref found,ref hit,new SliceHit {Id=frame.RenderedResidentIds[instance],Resident=true,Distance=distance});
            }
            return found;
        }
    }
}
