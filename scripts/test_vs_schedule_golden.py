"""Independent schedule oracle must match new committed Tier-1 fixtures."""
import unittest
import vs_schedule_golden as oracle
class ScheduleGoldenTest(unittest.TestCase):
    def test_independent_oracle_matches_committed_fixture(self):
        self.assertEqual(oracle.OUTPUT.read_text(),oracle.build())
