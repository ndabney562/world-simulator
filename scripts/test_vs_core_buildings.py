"""The compiled baseline must preserve the reviewed extract exactly."""
import json
import unittest
from decimal import Decimal
import vs_core_buildings as vc


class CoreBuildingsTest(unittest.TestCase):
    def test_compiled_constants_match_every_extract_row_byte_for_byte(self):
        self.assertEqual(vc.OUTPUT.read_text(encoding="utf-8"), vc.build())

    def test_scaling_preserves_all_source_decimals_without_rounding(self):
        self.assertEqual(vc.scaled(Decimal("-74.031111017822"), 10**12), -74031111017822)
        self.assertEqual(vc.scaled(Decimal("8.07"), 1000), 8070)
        for value in [Decimal("0.0000000000001"), Decimal("NaN"), Decimal("Infinity")]:
            with self.assertRaises(ValueError):
                vc.scaled(value, 10**12)

    def test_duplicate_source_bin_is_rejected(self):
        data = json.loads(vc.SOURCE.read_text())
        data["features"].append(data["features"][0])
        with self.assertRaisesRegex(ValueError, "duplicate BIN"):
            vc.build(json.dumps(data).encode())

    def test_holes_and_multipolygon_parts_and_closure_are_retained(self):
        data = json.loads(vc.SOURCE.read_text(), parse_float=str)
        f = data["features"][0]
        ring = [["-74.0", "40.0"], ["-73.9", "40.0"], ["-73.9", "40.1"], ["-74.0", "40.0"]]
        f["geometry"]["coordinates"] = [[ring, list(reversed(ring))], [ring]]
        generated = vc.build(json.dumps({"features": [f]}).encode())
        self.assertEqual(generated.count("Polygon { rings:"), 2)
        self.assertEqual(generated.count("Ring { points:"), 3)
        self.assertEqual(generated.count("lon_pdeg: -74000000000000, lat_pdeg: 40000000000000"), 6)
        f["geometry"]["coordinates"][0][0] = ring[:-1]
        with self.assertRaisesRegex(ValueError, "closed ring"):
            vc.build(json.dumps({"features": [f]}).encode())
