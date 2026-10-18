"""Tests for the Vertical Slice block extract (VS-001). Run from the repository root:
python3 -m unittest discover -s scripts -p 'test_*.py' -v
"""

import csv
import json
import unittest

import vs_block_extract as vx


class BlockExtractTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.features = json.loads((vx.BLOCK_DIR / "buildings.geojson").read_text(encoding="utf-8"))["features"]
        with open(vx.BLOCK_DIR / "lots.csv", encoding="utf-8", newline="") as fh:
            cls.lots = list(csv.DictReader(fh))

    def test_extract_regenerates_byte_for_byte_from_raw(self):
        for name, text in vx.build_outputs().items():
            self.assertEqual((vx.BLOCK_DIR / name).read_text(encoding="utf-8"), text, name)

    def test_every_building_has_a_closed_footprint_and_a_height(self):
        buildings = [f for f in self.features if f["properties"]["feature_code"] == "2100"]
        self.assertGreater(len(buildings), 0)
        for f in self.features:
            p = f["properties"]
            self.assertEqual(f["geometry"]["type"], "MultiPolygon", p["bin"])
            for polygon in f["geometry"]["coordinates"]:
                for ring in polygon:
                    self.assertGreaterEqual(len(ring), 4, p["bin"])
                    self.assertEqual(ring[0], ring[-1], p["bin"])
                    for lon, lat in ring:
                        self.assertTrue(-74.04 < lon < -74.02 and 40.62 < lat < 40.63, p["bin"])
            self.assertIsNotNone(p["height_roof_m"], p["bin"])
            self.assertGreater(p["height_roof_m"], 0, p["bin"])

    def test_every_lot_has_a_footprint(self):
        bbls = {f["properties"]["bbl"] for f in self.features}
        self.assertEqual(len(self.lots), 51)
        for lot in self.lots:
            self.assertIn(lot["bbl"], bbls, lot["address"])

    def test_featured_buildings_are_present_with_their_roles(self):
        featured = {f["properties"]["bin"]: f["properties"] for f in self.features if f["properties"]["featured"]}
        self.assertEqual({b: p["featured"] for b, p in featured.items()}, vx.FEATURED)
        for p in featured.values():
            self.assertEqual(p["feature_code"], "2100")
            self.assertIsNotNone(p["bldgclass"])
            self.assertGreater(p["unitsres"], 0)

    def test_centroid_lies_inside_the_footprint_bounds(self):
        for f in self.features:
            p = f["properties"]
            ring = f["geometry"]["coordinates"][0][0]
            lons = [c[0] for c in ring]
            lats = [c[1] for c in ring]
            self.assertTrue(min(lons) <= p["centroid_lon"] <= max(lons), p["bin"])
            self.assertTrue(min(lats) <= p["centroid_lat"] <= max(lats), p["bin"])

    def test_centroid_of_a_square(self):
        square = {"coordinates": [[[[0, 0], [2, 0], [2, 2], [0, 2], [0, 0]]]]}
        self.assertEqual(vx.centroid(square), (1.0, 1.0))


if __name__ == "__main__":
    unittest.main()
