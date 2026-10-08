import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location(
    "postgres_mutations", Path(__file__).with_name("check-postgres-mutations.py"))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


class PostgresMutationChecks(unittest.TestCase):
    def test_native_query_logger_fields_ignore_ansi_and_unrelated_messages(self):
        text = ("INFO GPU query result \x1b[3mquery\x1b[0m=input-configuration result={}\n"
                "INFO GPU query result query=ui-gpus result={}\n"
                "query=not-a-result result={}\n")
        self.assertEqual(probe.logged_queries(text), {"input-configuration", "ui-gpus"})

    def test_mirror_reads_one_aggregate_and_compares_every_table(self):
        instance = probe.Probe.__new__(probe.Probe)
        instance.latest = {}
        expected = {table: [{"id": table}] for table in probe.INPUTS.values()}
        records = [{"table": table, "value": rows[0]} for table, rows in expected.items()]
        instance.database = lambda: expected
        calls = []
        def rows(query):
            calls.append(query)
            return [{"records": records}]
        instance.rows = rows
        self.assertTrue(instance.mirrored())
        self.assertEqual(calls, [probe.DATABASE_QUERY])
        records.pop()
        self.assertFalse(instance.mirrored())
        records.append({"table": "unknown", "value": {}})
        with self.assertRaises(AssertionError):
            instance.mirrored()

    def test_row_comparison_preserves_multiplicity_and_types(self):
        self.assertTrue(probe.same_rows([{"x": 1}, {"x": 2}], [{"x": 2}, {"x": 1}]))
        self.assertFalse(probe.same_rows([{"x": 1}, {"x": 1}], [{"x": 1}]))
        self.assertFalse(probe.same_rows([{"x": True}], [{"x": 1}]))
        self.assertFalse(probe.same_rows([{"x": "1"}], [{"x": 1}]))

    def test_duplicate_or_missing_ui_identity_is_a_failure(self):
        probe.unique_rows("ui-gpus", [{"gpu_id": "a"}, {"gpu_id": "b"}])
        for rows in [[{"gpu_id": "a"}, {"gpu_id": "a"}], [{}], [{"gpu_id": ""}]]:
            with self.assertRaises(AssertionError):
                probe.unique_rows("ui-gpus", rows)

    def test_presenter_mutations_are_rejected_before_execution(self):
        instance = probe.Probe.__new__(probe.Probe)
        with self.assertRaisesRegex(AssertionError, "presenter is read-only"):
            instance.sql("UPDATE gpu_telemetry SET powered_on=false;", source=True, mutation=True)

    def test_sql_values_are_passed_as_quoted_psql_variables(self):
        instance = probe.Probe.__new__(probe.Probe)
        instance.compose = ["docker", "compose", "--project-name", "isolated"]
        calls = []
        instance.run = lambda command, **options: calls.append((command, options)) or b""
        value = "untrusted'; DROP TABLE gpu_inventory; --"
        instance.sql("SELECT :'value';", variables={"value": value})
        self.assertEqual(calls[0][1]["data"], b"SELECT :'value';")
        self.assertIn("value=" + value, calls[0][0])

    def test_inventory_covers_all_fourteen_queries(self):
        self.assertEqual(len(probe.QUERIES), 14)
        self.assertEqual(len(set(probe.QUERIES)), 14)
        self.assertEqual(len(probe.INPUTS), 7)
        self.assertEqual(len(probe.UI_KEYS), 9)

    def test_ephemeral_ports_are_resolved_again_after_restart(self):
        instance = probe.Probe.__new__(probe.Probe)
        instance.compose = ["docker", "compose", "--project-name", "isolated"]
        addresses = iter([b"127.0.0.1:54001\n", b"127.0.0.1:54002\n",
                          b"127.0.0.1:54003\n", b"127.0.0.1:54002\n"])
        instance.run = lambda command: next(addresses)
        instance.event = lambda kind, **details: None
        instance.refresh_urls()
        self.assertEqual(instance.drasi, "http://127.0.0.1:54001")
        instance.refresh_urls()
        self.assertEqual(instance.drasi, "http://127.0.0.1:54003")
        self.assertEqual(instance.control, "http://127.0.0.1:54002")

    def test_recovery_finding_distinguishes_unavailable_source(self):
        before = {"inputs_ready": True}
        after = {
            "inputs_ready": False,
            "components": [{"component_id": name, "status": "running"}
                           for name in ("postgres", probe.DATABASE_QUERY)]
                          + [{"component_id": "simulator", "status": "initialization-error"}],
        }
        self.assertTrue(probe.recovery_readiness_regressed(before, after))
        after["inputs_ready"] = True
        self.assertFalse(probe.recovery_readiness_regressed(before, after))
        after["inputs_ready"] = False
        after["components"][0]["status"] = "unavailable"
        self.assertFalse(probe.recovery_readiness_regressed(before, after))


if __name__ == "__main__":
    unittest.main()
