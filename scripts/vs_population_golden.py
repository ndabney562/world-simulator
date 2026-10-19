"""Independent integer SipHash oracle for VS-003 fixtures; never imports Rust.
Run with `check` (default) or `build` after an approved generator change.
"""
from pathlib import Path
from decimal import Decimal
import hashlib
import json
import sys

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "data/vertical-slice/bay-ridge-block-6049/buildings.geojson"
OUTPUT = ROOT / "core/crates/ws-world/tests/golden/block_6049_population.txt"
MASK = (1 << 64) - 1
GIVEN = "Alex Blair Casey Drew Eden Finley Gray Harper Jamie Kai Morgan Quinn Reese Rowan Sam Taylor".split()
FAMILY = "Adams Baker Chen Diaz Evans Flores Green Hall Ibrahim Jones Kim Lee Patel Rivera Singh Wong".split()

def purpose(label):
    h = 0xcbf29ce484222325
    for b in label.encode(): h = ((h ^ b) * 0x100000001b3) & MASK
    return h

def sip(k0, k1, words):
    v = [0x736f6d6570736575 ^ k0, 0x646f72616e646f6d ^ k1, 0x6c7967656e657261 ^ k0, 0x7465646279746573 ^ k1]
    def rol(x,b): return ((x << b) | (x >> (64-b))) & MASK
    def rnd():
        v[0]=(v[0]+v[1])&MASK; v[1]=rol(v[1],13)^v[0]; v[0]=rol(v[0],32)
        v[2]=(v[2]+v[3])&MASK; v[3]=rol(v[3],16)^v[2]
        v[0]=(v[0]+v[3])&MASK; v[3]=rol(v[3],21)^v[0]
        v[2]=(v[2]+v[1])&MASK; v[1]=rol(v[1],17)^v[2]; v[2]=rol(v[2],32)
    for word in list(words) + [(len(words)*8) << 56]:
        v[3]^=word; rnd(); rnd(); v[0]^=word
    v[2]^=255
    for _ in range(4): rnd()
    return v[0]^v[1]^v[2]^v[3]

def entity(seed,label,words): return (1<<62) | (sip(seed,purpose(label),words)>>2)
def draw(seed,label,id,count):
    key=sip(seed,purpose(label),[id])
    return (sip(key,purpose(label),[0])*count)>>64

def build():
    data=SOURCE.read_bytes()
    version=int.from_bytes(hashlib.sha256(data).digest()[:8],"big")
    features=json.loads(data,parse_float=Decimal)["features"]
    # Validate this separate oracle against the previously reviewed building IDs.
    for row in (ROOT/"core/crates/ws-world/tests/golden/block_6049.txt").read_text().splitlines():
        if row and not row.startswith("#"):
            seed,bin_id,id,*_=row.split()
            assert entity(int(seed,16),"ws-world/id/building",[version,int(bin_id)])==int(id,16)
    points=[point for f in features for polygon in f["geometry"]["coordinates"] for ring in polygon for point in ring]
    west=min(int(p[0]*10**12) for p in points); east=max(int(p[0]*10**12) for p in points)
    lat=(min(int(p[1]*10**12) for p in points)+max(int(p[1]*10**12) for p in points))//2
    out=[]
    for seed in [0,42,MASK]:
        buildings=sorted((entity(seed,"ws-world/id/building",[version,int(f["properties"]["bin"])]),f["properties"]) for f in features if f["properties"]["featured"])
        homes=[]; people=[]
        for home,b in buildings:
            bin_id=int(b["bin"])
            for unit in range(int(b["unitsres"])):
                hh=entity(seed,"ws-world/id/household",[version,bin_id,unit])
                family=FAMILY[draw(seed,"ws-world/slice/family-name",hh,16)]
                members=[]
                for slot in range(1+draw(seed,"ws-world/slice/household-size",hh,3)):
                    flat=unit*3+slot; id=entity(seed,"ws-world/id/resident",[version,bin_id,flat]); members.append(id)
                    age=(22+draw(seed,"ws-world/slice/age",id,59)) if slot==0 else draw(seed,"ws-world/slice/age",id,91)
                    role=0 if age<5 else 1 if age<22 else 2 if age<65 else 3
                    people.append([id,hh,home,bin_id,flat,age,role,GIVEN[draw(seed,"ws-world/slice/given-name",id,16)],family,0])
                homes.append((hh,home,bin_id,unit,members))
        shop_home,shop=next((id,b) for id,b in buildings if b["bldgclass"]=="S1")
        local=entity(seed,"ws-world/id/workplace",[version,int(shop["bin"]),0]); job=entity(seed,"ws-world/id/workplace",[version,0,0]); school=entity(seed,"ws-world/id/workplace",[version,0,1])
        capacity=max(1,min(3,int(shop["numfloors"])))
        employed=0
        for p in sorted(people):
            if p[6]==2:
                p[-1]=local if employed<capacity else job
                employed+=1
            elif p[6]==1: p[-1]=school
        out.append(f"world {seed:016x} {version:016x} 1 1")
        out.extend([f"exit 0 {west} {lat}",f"exit 1 {east} {lat}"])
        for id,kind,cap,home,exit in sorted([(local,0,capacity,shop_home,255),(job,0,30,0,0),(school,1,30,0,1)]): out.append(f"workplace {id:016x} {kind} {cap} {home:016x} {exit}")
        for id,home,bin_id,unit,members in sorted(homes): out.append(f"household {id:016x} {home:016x} {bin_id} {unit} "+",".join(f"{m:016x}" for m in members))
        for id,hh,home,bin_id,flat,age,role,given,family,work in sorted(people): out.append(f"resident {id:016x} {hh:016x} {home:016x} {bin_id} {flat} {age} {role} {given} {family} {work:016x}")
    return "\n".join(out)+"\n"

if __name__=="__main__":
    expected=build()
    if sys.argv[1:]==["build"]: OUTPUT.write_text(expected)
    elif sys.argv[1:] in ([],["check"]):
        if OUTPUT.read_text()!=expected: raise SystemExit("population golden is stale")
        print("population oracle/golden checks passed")
    else: raise SystemExit("usage: vs_population_golden.py [check|build]")
