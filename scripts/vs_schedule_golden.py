"""Independent VS-004 minute-table/jitter oracle, using the frozen VS-003 population fixture."""
from pathlib import Path
import sys
from vs_population_golden import draw, ROOT
OUTPUT=ROOT/"core/crates/ws-life/tests/golden/schedules.txt"

def table(role,weekend,j):
    if not weekend and role in (1,2):
        end=900 if role==1 else 1020
        rows=[(0,0),(390,5),(450,1),(480,3 if role==1 else 2),(end,1),(end+30,5),(1080,1),(1090,4),(1120,1),(1130,5),(1320,0)]
    else:
        rows=[(0,0),(480,5),(660,1),(670,4),(700,1),(710,5),(1320,0)]
    return ",".join(f"{m+j if m else 0}:{a}" for m,a in rows)

def build():
    result=[]
    for line in (ROOT/"core/crates/ws-world/tests/golden/block_6049_population.txt").read_text().splitlines():
        words=line.split()
        if words[0]=="world": seed=int(words[1],16)
        if words[0]!="resident":continue
        _,resident,household,home,bin_id,slot,age,role,given,family,work=words
        role=int(role);j=draw(seed,"ws-life/schedule/household-jitter",int(household,16),31)-15
        result.append(f"{seed:016x} {resident} {household} {role} {j} {table(role,False,j)} {table(role,True,j)}")
    return "\n".join(result)+"\n"

if __name__=="__main__":
    if sys.argv[1:]==["build"]:OUTPUT.parent.mkdir(parents=True,exist_ok=True);OUTPUT.write_text(build())
    elif sys.argv[1:] in ([],["check"]):
        if OUTPUT.read_text()!=build():raise SystemExit("schedule golden is stale")
        print("schedule oracle/golden checks passed")
    else:raise SystemExit("usage: vs_schedule_golden.py [check|build]")
