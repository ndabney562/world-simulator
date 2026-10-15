using System.Runtime.CompilerServices;

// Lets the edit-mode tests reach internal types once WorldSim.Rendering has some.
[assembly: InternalsVisibleTo("WorldSim.Tests.EditMode")]
