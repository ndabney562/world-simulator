#!/usr/bin/env python3
"""Vertical Slice block extract (VS-001, issue #65).

Two steps, so the committed extract regenerates byte-for-byte:

  fetch  Download the block's rows from NYC Open Data into raw/ (needs network).
         The city edits these datasets (footprints weekly, PLUTO quarterly), so a
         later fetch can differ; the committed raw/ files are the pinned snapshot.
  build  Turn raw/ into buildings.geojson and lots.csv. Offline and deterministic.

  python3 scripts/vs_block_extract.py fetch   # refresh the snapshot
  python3 scripts/vs_block_extract.py build   # rewrite the extract from raw/
  python3 scripts/vs_block_extract.py check   # fail if the extract is stale

Uses only the Python standard library. Sources are registered in docs/data/.
"""

import csv
import io
import json
import sys
import urllib.parse
import urllib.request
from decimal import Decimal
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BLOCK_DIR = ROOT / "data" / "vertical-slice" / "bay-ridge-block-6049"
RAW_DIR = BLOCK_DIR / "raw"

BOROUGH_CODE = 3  # Brooklyn
BLOCK = 6049
BBL_LO = f"{BOROUGH_CODE}{BLOCK:05d}0000"
BBL_HI = f"{BOROUGH_CODE}{BLOCK:05d}9999"

FOOTPRINTS_URL = "https://data.cityofnewyork.us/resource/5zhs-2jue.json"
PLUTO_URL = "https://data.cityofnewyork.us/resource/64uk-42ks.json"
# PLUTO columns kept. Owner names and assessed values are left out on purpose:
# the simulation does not need them.
PLUTO_FIELDS = [
    "bbl", "borough", "block", "lot", "address", "zipcode", "bldgclass", "landuse",
    "numbldgs", "numfloors", "unitsres", "unitstotal", "lotarea", "bldgarea",
    "comarea", "resarea", "retailarea", "lotfront", "lotdepth", "bldgfront",
    "bldgdepth", "yearbuilt", "zonedist1", "latitude", "longitude", "version",
]

FEET_TO_METRES = Decimal("0.3048")
FEATURE_TYPES = {
    "1000": "parking", "1001": "gas station canopy", "1002": "storage tank",
    "1003": "placeholder", "1004": "auxiliary structure", "1005": "temporary structure",
    "1006": "cantilevered building", "2100": "building", "2110": "skybridge",
    "5100": "building under construction", "5110": "garage",
}

# The 3 to 5 representative buildings named in ADR 0023, keyed by BIN.
FEATURED = {
    "3153708": "rowhouse",
    "3153715": "two-family house",
    "3153734": "small apartment building",
    "3153706": "shop with apartment above",
}

LOT_COLUMNS = [
    "bbl", "lot", "address", "bldgclass", "landuse", "numbldgs", "numfloors",
    "unitsres", "unitstotal", "lotarea_sqft", "bldgarea_sqft", "comarea_sqft",
    "resarea_sqft", "retailarea_sqft", "lotfront_ft", "lotdepth_ft", "bldgfront_ft",
    "bldgdepth_ft", "yearbuilt", "zonedist1", "latitude", "longitude", "pluto_version",
]


def soql(url, params):
    query = urllib.parse.urlencode(params)
    with urllib.request.urlopen(f"{url}?{query}", timeout=60) as resp:
        return json.load(resp)


def write_json(path, rows):
    path.write_text(json.dumps(rows, indent=1, sort_keys=True) + "\n", encoding="utf-8")


def fetch():
    RAW_DIR.mkdir(parents=True, exist_ok=True)
    where = f"base_bbl between '{BBL_LO}' and '{BBL_HI}'"
    footprints = soql(FOOTPRINTS_URL, {"$where": where, "$order": "bin,objectid", "$limit": 1000})
    where = f"borough='BK' AND block={BLOCK}"
    lots = soql(PLUTO_URL, {"$select": ",".join(PLUTO_FIELDS), "$where": where,
                            "$order": "lot", "$limit": 1000})
    write_json(RAW_DIR / "footprints.json", footprints)
    write_json(RAW_DIR / "pluto.json", lots)
    print(f"fetched {len(footprints)} footprints and {len(lots)} lots into {RAW_DIR.relative_to(ROOT)}")


def bbl10(value):
    """PLUTO gives BBL as '3060490010.00000000'; footprints give '3060490010'."""
    return str(int(Decimal(value)))


def num(value, places=None):
    """Decimal string from the API to a JSON number; None stays None."""
    if value in (None, ""):
        return None
    d = Decimal(value)
    if places is not None:
        d = d.quantize(Decimal(1).scaleb(-places))
    return int(d) if d == d.to_integral_value() else float(d)


def metres(feet):
    if feet in (None, ""):
        return None
    return num(str(Decimal(feet) * FEET_TO_METRES), 2)


def ring_centroid(ring):
    """Area-weighted centroid of a closed lon/lat ring (planar; fine at block scale).

    Coordinates are taken relative to the first vertex: raw lon/lat cross
    products cancel catastrophically on a 20 m footprint.
    """
    ox, oy = ring[0]
    a = cx = cy = 0.0
    for (x0, y0), (x1, y1) in zip(ring, ring[1:]):
        x0, y0, x1, y1 = x0 - ox, y0 - oy, x1 - ox, y1 - oy
        cross = x0 * y1 - x1 * y0
        a += cross
        cx += (x0 + x1) * cross
        cy += (y0 + y1) * cross
    a *= 0.5
    return a, ox + cx / (6 * a), oy + cy / (6 * a)


def centroid(multipolygon):
    total = sx = sy = 0.0
    for polygon in multipolygon["coordinates"]:
        for i, ring in enumerate(polygon):
            a, x, y = ring_centroid(ring)
            a = abs(a) if i == 0 else -abs(a)  # holes subtract
            total += a
            sx += a * x
            sy += a * y
    return round(sx / total, 6), round(sy / total, 6)


def lot_row(lot):
    return {
        "bbl": bbl10(lot["bbl"]),
        "lot": int(lot["lot"]),
        "address": lot.get("address", ""),
        "bldgclass": lot.get("bldgclass", ""),
        "landuse": lot.get("landuse", ""),
        "numbldgs": num(lot.get("numbldgs")),
        "numfloors": num(lot.get("numfloors")),
        "unitsres": num(lot.get("unitsres")),
        "unitstotal": num(lot.get("unitstotal")),
        "lotarea_sqft": num(lot.get("lotarea")),
        "bldgarea_sqft": num(lot.get("bldgarea")),
        "comarea_sqft": num(lot.get("comarea")),
        "resarea_sqft": num(lot.get("resarea")),
        "retailarea_sqft": num(lot.get("retailarea")),
        "lotfront_ft": num(lot.get("lotfront")),
        "lotdepth_ft": num(lot.get("lotdepth")),
        "bldgfront_ft": num(lot.get("bldgfront")),
        "bldgdepth_ft": num(lot.get("bldgdepth")),
        "yearbuilt": num(lot.get("yearbuilt")),
        "zonedist1": lot.get("zonedist1", ""),
        "latitude": num(lot.get("latitude")),
        "longitude": num(lot.get("longitude")),
        "pluto_version": lot.get("version", ""),
    }


def build_outputs():
    """Return {filename: text} for the extract, built only from raw/."""
    footprints = json.loads((RAW_DIR / "footprints.json").read_text(encoding="utf-8"))
    pluto = json.loads((RAW_DIR / "pluto.json").read_text(encoding="utf-8"))
    lots = sorted((lot_row(l) for l in pluto), key=lambda r: r["lot"])
    by_bbl = {r["bbl"]: r for r in lots}

    features = []
    for f in sorted(footprints, key=lambda f: (int(f["bin"]), int(f["objectid"]))):
        lot = by_bbl.get(f["base_bbl"], {})
        lon, lat = centroid(f["the_geom"])
        props = {
            "bin": f["bin"],
            "bbl": f["base_bbl"],
            "lot": lot.get("lot"),
            "address": lot.get("address"),
            "feature_code": f.get("feature_code"),
            "feature_type": FEATURE_TYPES.get(f.get("feature_code"), "unknown"),
            "height_roof_ft": num(f.get("height_roof")),
            "height_roof_m": metres(f.get("height_roof")),
            "ground_elevation_ft_navd88": num(f.get("ground_elevation")),
            "ground_elevation_m_navd88": metres(f.get("ground_elevation")),
            "construction_year": num(f.get("construction_year")),
            "geom_source": f.get("geom_source"),
            "last_edited_date": f.get("last_edited_date"),
            "bldgclass": lot.get("bldgclass"),
            "numfloors": lot.get("numfloors"),
            "unitsres": lot.get("unitsres"),
            "centroid_lon": lon,
            "centroid_lat": lat,
            "featured": FEATURED.get(f["bin"]),
        }
        features.append({"type": "Feature", "properties": props, "geometry": f["the_geom"]})

    lines = ['{"type":"FeatureCollection","features":[']
    for i, feature in enumerate(features):
        sep = "," if i < len(features) - 1 else ""
        lines.append(json.dumps(feature, sort_keys=True, separators=(",", ":")) + sep)
    lines.append("]}")
    geojson = "\n".join(lines) + "\n"

    out = io.StringIO()
    writer = csv.DictWriter(out, fieldnames=LOT_COLUMNS, lineterminator="\n")
    writer.writeheader()
    for row in lots:
        writer.writerow({k: "" if row[k] is None else row[k] for k in LOT_COLUMNS})
    return {"buildings.geojson": geojson, "lots.csv": out.getvalue()}


def build():
    for name, text in build_outputs().items():
        (BLOCK_DIR / name).write_text(text, encoding="utf-8")
        print(f"wrote {(BLOCK_DIR / name).relative_to(ROOT)}")


def check():
    stale = [n for n, t in build_outputs().items()
             if (BLOCK_DIR / n).read_text(encoding="utf-8") != t]
    for name in stale:
        print(f"{(BLOCK_DIR / name).relative_to(ROOT)} is stale; run: python3 scripts/vs_block_extract.py build")
    return 1 if stale else 0


def main(argv):
    commands = {"fetch": fetch, "build": build, "check": check}
    if len(argv) != 2 or argv[1] not in commands:
        print(__doc__)
        return 2
    return commands[argv[1]]() or 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
