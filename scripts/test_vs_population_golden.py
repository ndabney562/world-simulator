"""The independent VS-003 oracle must match committed full-population fixtures."""
import unittest
import vs_population_golden as oracle

class PopulationGoldenTest(unittest.TestCase):
    def test_independent_population_oracle_matches_committed_fixture(self):
        self.assertEqual(oracle.OUTPUT.read_text(), oracle.build())
