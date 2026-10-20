"""Independent ADR0028 graph/path/need oracle. New version-1 fixtures only.

Reads pinned source and previously frozen population IDs; path search enumerates
all simple paths rather than repeating Rust's Dijkstra implementation.
"""
from decimal import Decimal
import json
import sys
from vs_population_golden import ROOT, MASK, purpose, sip

OUTPUT = ROOT / "core/crates/ws-life/tests/golden/behavior.txt"


def stream(seed, label, words, counter):
    key = sip(seed, purpose(label), words)
    return sip(key, purpose(label), [counter])


def build():
    features = json.loads((ROOT / "data/vertical-slice/bay-ridge-block-6049/buildings.geojson").read_text(), parse_float=Decimal)["features"]
    points = [(int(x*10**12), int(y*10**12)) for f in features for poly in f["geometry"]["coordinates"] for ring in poly for x, y in ring]
    origin = (min(x for x, y in points), min(y for x, y in points))

    def project(p):
        return ((p[0]-origin[0])*84_500_000//10**12, (p[1]-origin[1])*111_200_000//10**12)

    east = max(project(p)[0] for p in points) + 5000
    north = max(project(p)[1] for p in points) + 5000

    def portal(p):
        x, y = project(p)
        return min([(x+5000, 0, (-5000, y)), (east-x, 1, (east, y)), (y+5000, 2, (x, -5000)), (north-y, 3, (x, north))])[2]

    centroids = {int(f["properties"]["bin"]): (int(f["properties"]["centroid_lon"]*10**12), int(f["properties"]["centroid_lat"]*10**12)) for f in features if f["properties"]["featured"]}
    worlds = {}
    for line in (ROOT / "core/crates/ws-world/tests/golden/block_6049_population.txt").read_text().splitlines():
        row = line.split()
        if row[0] == "world":
            seed = int(row[1], 16)
            worlds[seed] = dict(exits={}, homes={}, remote=[], people=[])
        elif row[0] == "exit":
            worlds[seed]["exits"][int(row[1])] = (int(row[2]), int(row[3]))
        elif row[0] == "household":
            worlds[seed]["homes"][int(row[2], 16)] = int(row[3])
        elif row[0] == "workplace" and row[4] == "0000000000000000":
            worlds[seed]["remote"].append((int(row[1], 16), int(row[5])))
        elif row[0] == "resident":
            worlds[seed]["people"].append(int(row[1], 16))
    result = ["navigation_rules 1 behavior_rules 1"]
    for seed, data in worlds.items():
        places = [(0, home, 255, portal(centroids[bin_id])) for home, bin_id in sorted(data["homes"].items())]
        places += [(1, work, exit, portal(data["exits"][exit])) for work, exit in sorted(data["remote"])]
        nodes = sorted(set([(-5000, -5000), (-5000, north), (east, -5000), (east, north)] + [p[3] for p in places]))
        edges = set()
        for axis, value in [(0, -5000), (0, east), (1, -5000), (1, north)]:
            side = [i for i, p in enumerate(nodes) if p[axis] == value]
            edges.update(zip(side, side[1:]))
        weight = lambda a, b: abs(nodes[a][0]-nodes[b][0])+abs(nodes[a][1]-nodes[b][1])
        neighbors = {i: [] for i in range(len(nodes))}
        for a, b in edges:
            neighbors[a].append(b)
            neighbors[b].append(a)

        def shortest(start, end):
            paths = []

            def walk(path, cost):
                if path[-1] == end:
                    paths.append((cost, tuple(path)))
                    return
                for next in neighbors[path[-1]]:
                    if next not in path:
                        walk(path+[next], cost+weight(path[-1], next))

            walk([start], 0)
            return min(paths)

        result.append(f"world {seed:016x} origin {origin[0]} {origin[1]} bounds -5000 {east} -5000 {north}")
        result += [f"node {i} {x} {y}" for i, (x, y) in enumerate(nodes)]
        result += [f"edge {a} {b} {weight(a,b)}" for a, b in sorted(edges)]
        result += [f"place {kind} {id:016x} {exit} {nodes.index(point)}" for kind, id, exit, point in places]
        for a in range(len(nodes)):
            for b in range(len(nodes)):
                distance, path = shortest(a, b)
                result.append(f"path {a} {b} {distance} "+",".join(map(str, path)))
        for id in data["people"]:
            needs = [(stream(seed, "ws-life/needs/initial", [id], k)*100_001)>>64 for k in range(4)]
            ties = [max(range(5), key=lambda action: (stream(seed, "ws-life/utility/tie", [id, action], counter), -action)) for counter in [0, 1, MASK]]
            result.append(f"needs {seed:016x} {id:016x} "+" ".join(map(str, needs+ties)))
    return "\n".join(result)+"\n"


if __name__ == "__main__":
    expected = build()
    if sys.argv[1:] == ["build"]:
        OUTPUT.write_text(expected)
    elif sys.argv[1:] in ([], ["check"]):
        if OUTPUT.read_text() != expected:
            raise SystemExit("behavior/path golden is stale")
        print("behavior/path oracle checks passed")
    else:
        raise SystemExit("usage: vs_behavior_golden.py [check|build]")
