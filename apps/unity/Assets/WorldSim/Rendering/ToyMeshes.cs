using System.Collections.Generic;
using UnityEngine;

namespace WorldSim.Rendering
{
    /// <summary>
    /// Unit box and capsule meshes built in code. GameObject.CreatePrimitive also adds a collider, and the iOS
    /// player strips physics colliders the project never references ("class 'CapsuleCollider' doesn't exist").
    /// Sizes match Unity's primitives: box 1 m, capsule 1 m wide and 2 m tall, both centred on the origin.
    /// </summary>
    public static class ToyMeshes
    {
        public static Mesh Box()
        {
            var vertices = new List<Vector3>();
            var triangles = new List<int>();
            foreach (var n in new[] { Vector3.right, Vector3.left, Vector3.up, Vector3.down, Vector3.forward, Vector3.back })
            {
                Vector3 up = Mathf.Abs(n.y) > 0.5f ? Vector3.forward : Vector3.up;
                Vector3 right = Vector3.Cross(up, -n); // viewer outside the face looks along -n
                int start = vertices.Count;
                vertices.Add((n - right - up) * 0.5f);
                vertices.Add((n - right + up) * 0.5f);
                vertices.Add((n + right + up) * 0.5f);
                vertices.Add((n + right - up) * 0.5f);
                triangles.AddRange(new[] { start, start + 1, start + 2, start, start + 2, start + 3 }); // clockwise seen from outside
            }
            return Build("Toy box", vertices, triangles);
        }

        public static Mesh Capsule(int segments = 16, int ringsPerCap = 6)
        {
            var vertices = new List<Vector3>();
            var triangles = new List<int>();
            int rings = 0;
            // Upper cap rings from the pole to the equator, then lower cap rings from the equator to the pole;
            // the two equator rings form the 1 m cylinder between the caps.
            for (int half = 0; half < 2; half++)
                for (int r = 0; r <= ringsPerCap; r++, rings++)
                {
                    float theta = Mathf.PI * 0.5f * (half + (float)r / ringsPerCap);
                    float y = Mathf.Cos(theta) * 0.5f + (half == 0 ? 0.5f : -0.5f);
                    float radius = Mathf.Sin(theta) * 0.5f;
                    for (int s = 0; s <= segments; s++)
                    {
                        float phi = 2 * Mathf.PI * s / segments;
                        vertices.Add(new Vector3(Mathf.Cos(phi) * radius, y, Mathf.Sin(phi) * radius));
                    }
                }
            int stride = segments + 1;
            for (int r = 0; r + 1 < rings; r++)
                for (int s = 0; s < segments; s++)
                {
                    int a = r * stride + s, b = a + 1, c = a + stride, d = c + 1;
                    triangles.AddRange(new[] { a, b, c, b, d, c }); // clockwise seen from outside
                }
            return Build("Toy capsule", vertices, triangles);
        }

        /// <summary>Flat two-sided annulus of unit outer radius in the XZ plane; scale X/Z to size it.</summary>
        public static Mesh Ring(float inner = 0.85f, int segments = 48)
        {
            var vertices = new List<Vector3>();
            var triangles = new List<int>();
            for (int s = 0; s <= segments; s++)
            {
                float phi = 2 * Mathf.PI * s / segments;
                var d = new Vector3(Mathf.Cos(phi), 0, Mathf.Sin(phi));
                vertices.Add(d * inner);
                vertices.Add(d);
            }
            for (int s = 0; s < segments; s++)
            {
                int a = s * 2, b = a + 1, c = a + 2, d = a + 3;
                triangles.AddRange(new[] { a, c, b, b, c, d, a, b, c, b, d, c }); // both windings: visible from any side
            }
            var mesh = Build("Toy ring", vertices, triangles);
            // Opposite windings cancel recalculated normals; the ring is lit as an upward face.
            var up = new Vector3[vertices.Count];
            for (int i = 0; i < up.Length; i++) up[i] = Vector3.up;
            mesh.normals = up;
            return mesh;
        }

        static Mesh Build(string name, List<Vector3> vertices, List<int> triangles)
        {
            var mesh = new Mesh { name = name };
            mesh.SetVertices(vertices);
            mesh.SetTriangles(triangles, 0);
            mesh.RecalculateNormals();
            mesh.RecalculateBounds();
            return mesh;
        }
    }
}
