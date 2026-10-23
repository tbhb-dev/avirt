import fnmatch
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def tracked():
    out = subprocess.run(
        ["git", "ls-files"], cwd=ROOT, check=True, capture_output=True, text=True
    )
    return out.stdout.splitlines()


def codeowners_patterns():
    lines = (ROOT / ".github/CODEOWNERS").read_text().splitlines()
    return [l.split()[0] for l in lines if l.strip() and not l.startswith("#")]


def owner_paths():
    for line in (ROOT / ".agents/agent-skills.conf").read_text().splitlines():
        if line.startswith("owner_paths"):
            return re.compile(line.split("=", 1)[1].strip())
    raise AssertionError("owner_paths not set")


def matches(pattern, path):
    pattern = pattern.lstrip("/")
    if pattern.endswith("/"):
        return path.startswith(pattern)
    return fnmatch.fnmatchcase(path, pattern)


def unmatched_patterns(patterns, paths):
    return [p for p in patterns if not any(matches(p, path) for path in paths)]


def patterns_outside_owner_paths(patterns, paths, regex):
    return [
        pattern
        for pattern in patterns
        if any(matches(pattern, path) and not regex.search(path) for path in paths)
    ]


def test_every_pattern_matches_a_tracked_path():
    assert unmatched_patterns(codeowners_patterns(), tracked()) == []


def test_codeowners_is_a_subset_of_owner_paths():
    assert (
        patterns_outside_owner_paths(codeowners_patterns(), tracked(), owner_paths())
        == []
    )


def test_a_pattern_matching_no_tracked_path_is_unmatched():
    assert unmatched_patterns(["/no-such-dir/"], tracked()) == ["/no-such-dir/"]


def test_a_pattern_matching_a_path_outside_owner_paths_is_reported():
    assert patterns_outside_owner_paths(["/README.md"], tracked(), owner_paths()) == [
        "/README.md"
    ]


def test_a_valid_pattern_inside_owner_paths_passes_both_checks():
    patterns = ["/.github/CODEOWNERS"]
    assert unmatched_patterns(patterns, tracked()) == []
    assert patterns_outside_owner_paths(patterns, tracked(), owner_paths()) == []
