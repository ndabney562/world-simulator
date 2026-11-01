using NUnit.Framework;
using UnityEngine;
using WorldSim.Rendering;

namespace WorldSim.Tests.EditMode
{
    // Moved unchanged from the deleted BridgePlayTests (ADR 0040): SliceCamera and SliceDemo
    // still read UnityEngine.Input, and the slice still draws with ToyMeshes.
    public sealed class SliceInputAndMeshTests
    {
        [Test]
        public void DemoInputBackendSupportsLegacyMouseAndTouch()
        {
#if ENABLE_LEGACY_INPUT_MANAGER
            Assert.DoesNotThrow(() => { var position = Input.mousePosition; var count = Input.touchCount; });
#else
            Assert.Fail("The demo uses UnityEngine.Input; select Input Manager in Active Input Handling.");
#endif
        }

        [Test]
        public void CodeBuiltMeshesMatchPrimitiveSizesAndFaceOutwards()
        {
            var box = ToyMeshes.Box();
            var capsule = ToyMeshes.Capsule();
            try
            {
                Assert.That(Vector3.Distance(box.bounds.size, Vector3.one), Is.LessThan(0.0001f));
                Assert.That(Vector3.Distance(capsule.bounds.size, new Vector3(1, 2, 1)), Is.LessThan(0.0001f));
                foreach (var mesh in new[] { box, capsule })
                {
                    var v = mesh.vertices; var t = mesh.triangles;
                    for (int i = 0; i < t.Length; i += 3)
                    {
                        Vector3 a = v[t[i]], b = v[t[i + 1]], c = v[t[i + 2]];
                        Vector3 normal = Vector3.Cross(b - a, c - a);
                        if (normal.sqrMagnitude < 1e-12f) continue; // degenerate pole triangles
                        Vector3 centre = (a + b + c) / 3;
                        Vector3 outward = centre - new Vector3(0, Mathf.Clamp(centre.y, -0.5f, 0.5f), 0);
                        if (mesh == box) outward = centre;
                        Assert.That(Vector3.Dot(normal, outward), Is.GreaterThan(0), $"{mesh.name} triangle {i / 3} faces inwards");
                    }
                }
            }
            finally { Object.DestroyImmediate(box); Object.DestroyImmediate(capsule); }
        }
    }
}
