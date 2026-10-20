"""Independent new Tier-1 rule/path fixture regeneration."""
import unittest
import vs_behavior_golden as oracle


class BehaviorGoldenTest(unittest.TestCase):
    def test_independent_path_enumeration_and_need_streams_match_fixture(self):
        self.assertEqual(oracle.OUTPUT.read_text(), oracle.build())
