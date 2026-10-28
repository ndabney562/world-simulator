"""Read clock evidence only after the real native loader validates this save."""
import json
import re
import sqlite3
import struct
import subprocess
import sys
from pathlib import Path
file=Path(sys.argv[2]).resolve()
native=subprocess.check_output([sys.argv[1],'load','--file',str(file),'--ticks','0'],text=True)
match=re.search(r'loaded tick=(\d+) state=([0-9a-f]+)',native)
assert match, native
with sqlite3.connect(file.as_uri()+'?mode=ro',uri=True) as db:
    version,tick,state=db.execute('select format_version,tick,state_hash from header where id=1').fetchone()
    schema,data=db.execute('select schema,data from clock where id=1').fetchone()
assert version==4 and schema in (1,2) and len(data)>=56
steps=struct.unpack_from('<Q',data,16)[0]
scale=struct.unpack_from('<I',data,24)[0]
assert steps==tick==int(match[1]) and scale in (0,1,10,100)
assert int(match[2],16)==state & ((1<<64)-1)
print(json.dumps({'tick':tick,'state':match[2],'scale':scale,'clock_schema':schema,'native_validated':True}))
