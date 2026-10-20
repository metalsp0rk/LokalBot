import importlib.util
import io
import json
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SAMPLE = ROOT / "LokalBot/Resources/SampleMeeting"
spec = importlib.util.spec_from_file_location("sample_meeting", ROOT / "Scripts/sample_meeting.py")
sample_meeting = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sample_meeting)


def load(name):
    return json.loads((SAMPLE / name).read_text(encoding="utf-8"))


class SampleMeetingFilesTests(unittest.TestCase):
    def test_reference_matches_the_script_and_audio(self):
        script, reference = load("script.en.json"), load("reference.en.json")
        self.assertTrue((SAMPLE / script["audio"]).is_file())
        self.assertEqual([t["id"] for t in reference["turns"]], [t["id"] for t in script["turns"]])
        self.assertEqual([t["text"] for t in reference["turns"]], [t["text"] for t in script["turns"]])
        starts = [t["start"] for t in reference["turns"]]
        self.assertEqual(starts, sorted(starts))
        self.assertTrue(all(t["end"] > t["start"] for t in reference["turns"]))
        self.assertTrue(180 <= reference["duration"] <= 300, "3 to 5 minutes")

    def test_checklist_points_at_turns_that_say_it(self):
        script, expected = load("script.en.json"), load("expected.en.json")
        turns = {t["id"]: t["text"].lower() for t in script["turns"]}
        self.assertEqual(expected["sampleID"], script["sampleID"])
        for entity in expected["entities"]:
            self.assertIn(entity["turn"], turns)
            self.assertTrue(any(v in turns[entity["turn"]] for v in entity["any"]),
                            f"{entity['label']} is not said in {entity['turn']}")
        for key in ("decision", "openQuestion"):
            self.assertTrue(set(expected[key]["turns"]) <= set(turns))
        changed = expected["changedDecision"]
        self.assertTrue(set(changed["beforeTurns"] + changed["afterTurns"]) <= set(turns))
        self.assertTrue(set(expected["ask"]["citesTurns"]) <= set(changed["afterTurns"]))


class ScoringTests(unittest.TestCase):
    def test_spoken_and_written_numbers_agree(self):
        normalize = sample_meeting.normalize
        for spoken, written in [
            ("forty-two partners", "42 partners"),
            ("November eighteenth", "November 18th"),
            ("release four point two", "release 4.2"),
            ("about a hundred and eighty milliseconds", "about 180 milliseconds"),
            ("two million open orders", "2,000,000 open orders"),
            ("p95 latency", "P 95 latency"),
        ]:
            self.assertEqual(normalize(spoken, "en"), normalize(written, "en"), written)

    def test_the_reference_scores_perfectly_against_itself(self):
        reference = load("reference.en.json")
        segments = [{"start": t["start"], "end": t["end"], "speaker": t["speaker"], "text": t["text"]}
                    for t in reference["turns"]]
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "transcript.json"
            path.write_text(json.dumps({"segments": segments}), encoding="utf-8")
            args = type("Args", (), {"lang": "en", "transcript": directory, "summary": None})
            out = io.StringIO()
            with redirect_stdout(out):
                sample_meeting.check(args)
        report = out.getvalue()
        self.assertIn("WER: 0.0%", report)
        self.assertIn(f"{len(load('expected.en.json')['entities'])}/", report)
        self.assertNotIn("✗", report)
        self.assertIn("100% of speech time on the right voice", report)


if __name__ == "__main__":
    unittest.main()
