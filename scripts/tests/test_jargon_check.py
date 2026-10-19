"""Tests for scripts/jargon_check.py. Run: mise run test-jargon"""

import subprocess
import sys
from pathlib import Path

import jargon_check as jc
import pytest


def prose(md: str) -> list[str]:
    return [w.raw for w in jc.prose_words(md) if w is not None]


# --- skipped regions -------------------------------------------------------------------------


@pytest.mark.parametrize(
    "md, gone",
    [
        ("Text\n\n```rust\nfn wombat_gadget() {}\n```\n", "wombat"),
        ("Text\n\n    indented zebra widget\n", "zebra"),
        ("Use `quokka-thing` here.", "quokka"),
        ("See [the docs](https://example.com/yak-shave) now.", "yak"),
        ("---\ntitle: Frontmatter narwhal\n---\n\nBody.", "narwhal"),
        ("Before <!-- hidden okapi --> after.", "okapi"),
        ("<div class=\"ibex\">\n\nBody.\n\n</div>", "ibex"),
        ("| a | b |\n|---|---|\n| `tapir-cell` | text |\n", "tapir"),
        ("See docs/some-path/lemur.md and v1.2.3 and deadbeefcafe1234 and 30 ms and 5MB.", "lemur"),
        ("A bare https://example.com/gnu-thing url.", "gnu"),
    ],
)
def test_skipped_region(md, gone):
    assert not any(gone in w for w in prose(md))


def test_link_text_kept_and_unit_residue_dropped():
    words = prose("See [the stoat guide](./x.md) every 30 s.")
    assert "stoat" in words and "s" not in words


def test_skipped_region_breaks_phrases():
    groups = [[w.raw for w in s] for s in jc.segments(jc.prose_words("alpha `code` beta"))]
    assert groups == [["alpha"], ["beta"]]


def test_added_filter_keeps_only_added_lines():
    md = "old line here\n\nnew line there\n"
    assert [w.raw for w in jc.prose_words(md, {3}) if w] == ["new", "line", "there"]


def test_possessive_and_plural_fold():
    assert jc.norm_term("The mediators' Workload's budgets") == "the mediator workload budget"
    assert jc.fold("processes") == "process" and jc.fold("refuses") == "refuse"


@pytest.mark.parametrize("singular, plural", [("cache", "caches"), ("size", "sizes"), ("box", "boxes"), ("class", "classes")])
def test_fold_gives_singular_and_plural_one_form(singular, plural):
    assert jc.fold(singular) == jc.fold(plural)


# --- detection ---------------------------------------------------------------------------------


def make_repo(tmp_path, monkeypatch, base: dict[str, str]):
    monkeypatch.chdir(tmp_path)

    def run_git(*a):
        return subprocess.run(["git", *a], cwd=tmp_path, check=True, capture_output=True)

    run_git("init", "-q", "-b", "main")
    run_git("config", "user.email", "t@example.com")
    run_git("config", "user.name", "t")
    write(tmp_path, base)
    run_git("add", "-A")
    run_git("commit", "-q", "-m", "base")
    return run_git


def write(root: Path, files: dict[str, str]):
    for name, text in files.items():
        (root / name).parent.mkdir(parents=True, exist_ok=True)
        (root / name).write_text(text)


BASE = {"docs/a.md": "# A\n\nThe workload runs one process and a mediator.\n"}


def terms(tmp_path, monkeypatch, head: dict[str, str], base=BASE, mode="tree"):
    git = make_repo(tmp_path, monkeypatch, base)
    write(tmp_path, head)
    if mode == "ref":
        git("add", "-A")
        git("commit", "-q", "-m", "head")
        findings, summary = jc.run("main~1", "HEAD", False)
    elif mode == "staged":
        git("add", "-A")
        write(tmp_path, {"docs/unstaged.md": "# U\n\nA walrus-ledger appears.\n"})
        findings, summary = jc.run("main", None, True)
    else:
        findings, summary = jc.run("main", None, False)
    return findings, summary


def names(findings):
    return sorted(f.term for f in findings)


NEW = "# B\n\nThe drift-gauge sits left. Another drift-gauge sits right.\n"


def test_new_hyphen_compound_flagged_on_first_use(tmp_path, monkeypatch):
    findings, _ = terms(tmp_path, monkeypatch, {"docs/b.md": "# B\n\nOne drift-gauge here.\n"})
    assert names(findings) == ["drift-gauge"] and findings[0].line == 3


def test_compound_in_base_is_known(tmp_path, monkeypatch):
    base = {"docs/a.md": "# A\n\nThe drift-gauge exists.\n"}
    findings, _ = terms(tmp_path, monkeypatch, {"docs/b.md": NEW}, base=base)
    assert findings == []


def test_compound_known_as_spaced_words(tmp_path, monkeypatch):
    base = {"docs/a.md": "# A\n\nThe edge proxy runs.\n"}
    findings, _ = terms(tmp_path, monkeypatch, {"docs/b.md": "# B\n\nThe edge-proxy runs.\n"}, base=base)
    assert findings == []


def test_code_identifier_in_base_counts_as_known(tmp_path, monkeypatch):
    base = {"docs/a.md": "# A\n\nText.\n", "src/lib.rs": "fn gauge_drift() {}\n", "crates/x/Cargo.toml": 'name = "drift-gauge"\n'}
    findings, _ = terms(tmp_path, monkeypatch, {"docs/b.md": "# B\n\nOne drift-gauge here, one drift gauge there.\n"}, base=base)
    assert findings == []


@pytest.mark.parametrize("compound", ["read-only", "long-term", "non-empty", "well-known", "built-in"])
def test_ordinary_compounds_pass(tmp_path, monkeypatch, compound):
    findings, _ = terms(tmp_path, monkeypatch, {"docs/b.md": f"# B\n\nA {compound} thing.\n"})
    assert findings == []


def test_phrase_needs_two_uses(tmp_path, monkeypatch):
    once = "# B\n\nThe drift budget matters.\n"
    twice = once + "\nThe drift budget again.\n"
    (tmp_path / "1").mkdir()
    findings, _ = terms(tmp_path / "1", monkeypatch, {"docs/b.md": once})
    assert findings == []
    (tmp_path / "2").mkdir()
    findings, _ = terms(tmp_path / "2", monkeypatch, {"docs/b.md": twice})
    assert names(findings) == ["drift budget"] and findings[0].uses == 2


def test_uses_count_across_files_and_report_once_per_file(tmp_path, monkeypatch):
    head = {
        "docs/b.md": "# B\n\nThe drift budget matters.\n\nThe drift budget again.\n",
        "docs/c.md": "# C\n\nA drift budget here.\n",
    }
    findings, _ = terms(tmp_path, monkeypatch, head)
    assert sorted((f.path, f.uses) for f in findings) == [("docs/b.md", 2), ("docs/c.md", 1)]


@pytest.mark.parametrize("phrase", ["the drift budget", "drift budget of", "to drift budget"])
def test_stopword_edge_is_never_part_of_a_reported_phrase(tmp_path, monkeypatch, phrase):
    text = f"# B\n\nZed {phrase} end. Yan {phrase} end.\n"
    findings, _ = terms(tmp_path, monkeypatch, {"docs/b.md": text})
    assert names(findings) == ["drift budget"]


def test_interior_stopword_phrase_is_skipped(tmp_path, monkeypatch):
    text = "# B\n\nZed drift and budget end. Yan drift and budget end.\n"
    findings, _ = terms(tmp_path, monkeypatch, {"docs/b.md": text})
    assert findings == []


def test_verb_fragments_and_common_phrases_skip(tmp_path, monkeypatch):
    text = "# B\n\nThe proxy omits things. The proxy omits things. A big house. A big house.\n"
    findings, _ = terms(tmp_path, monkeypatch, {"docs/b.md": text})
    assert findings == []


def test_longest_phrase_wins_when_shorter_never_stands_alone(tmp_path, monkeypatch):
    text = "# B\n\nThe quartz drift budget. The quartz drift budget.\n"
    findings, _ = terms(tmp_path, monkeypatch, {"docs/b.md": text})
    assert names(findings) == ["quartz drift budget"]


def test_shorter_phrase_survives_when_used_alone(tmp_path, monkeypatch):
    text = "# B\n\nThe quartz drift budget. The quartz drift budget. The drift budget.\n"
    findings, _ = terms(tmp_path, monkeypatch, {"docs/b.md": text})
    assert "drift budget" in names(findings)


def test_code_block_use_does_not_count(tmp_path, monkeypatch):
    text = "# B\n\nThe `drift budget` one. Also ```drift budget``` two.\n\n```\ndrift budget\ndrift budget\n```\n"
    findings, _ = terms(tmp_path, monkeypatch, {"docs/b.md": text})
    assert findings == []


def test_only_added_lines_are_read(tmp_path, monkeypatch):
    base = {"docs/b.md": "# B\n\nThe drift budget old. The drift budget old.\n"}
    head = {"docs/b.md": "# B\n\nThe drift budget old. The drift budget old.\n\nPlain addition.\n"}
    findings, _ = terms(tmp_path, monkeypatch, head, base=base)
    assert findings == []


# --- allowed terms -----------------------------------------------------------------------------

TWICE = "# B\n\nThe drift budget matters. The drift budget again.\n"
GLOSSARY = "# Terms\n\n## Drift budget\n\nHow far the replica may lag.\n"


def test_glossary_heading_allows_term(tmp_path, monkeypatch):
    findings, summary = terms(tmp_path, monkeypatch, {"docs/b.md": TWICE, "docs/glossary.md": GLOSSARY})
    assert findings == []
    assert any("glossary entries added: 1" in s for s in summary)


def test_glossary_entry_in_base_allows_term(tmp_path, monkeypatch):
    findings, _ = terms(tmp_path, monkeypatch, {"docs/b.md": TWICE}, base={**BASE, "docs/glossary.md": GLOSSARY})
    assert findings == []


def test_accept_txt_pattern_allows_term(tmp_path, monkeypatch):
    accept = ".vale/config/vocabularies/project/accept.txt"
    findings, summary = terms(tmp_path, monkeypatch, {"docs/b.md": "# B\n\nA drift-gauge.\n", accept: "[Dd]rift-gauge\n"})
    assert findings == [] and any("accept.txt entries added: 1" in s for s in summary)


def test_allow_file_with_reason_allows_and_is_counted(tmp_path, monkeypatch):
    findings, summary = terms(tmp_path, monkeypatch, {"docs/b.md": TWICE, "jargon-accept.txt": "drift budget  # maintainer approved\n"})
    assert findings == []
    assert any("allow entries added: 1" in s for s in summary)


def test_allow_entry_without_reason_is_a_finding_and_does_not_allow(tmp_path, monkeypatch):
    findings, _ = terms(tmp_path, monkeypatch, {"docs/b.md": TWICE, "jargon-accept.txt": "# header\n\ndrift budget\nother term #\n"})
    assert sorted((f.kind, f.term) for f in findings) == [
        ("missing-reason", "drift budget"),
        ("missing-reason", "other term"),
        ("new-term", "drift budget"),
    ]


# --- modes -------------------------------------------------------------------------------------


def test_ref_mode(tmp_path, monkeypatch):
    findings, _ = terms(tmp_path, monkeypatch, {"docs/b.md": TWICE}, mode="ref")
    assert names(findings) == ["drift budget"]


def test_staged_mode_ignores_unstaged_files(tmp_path, monkeypatch):
    findings, _ = terms(tmp_path, monkeypatch, {"docs/b.md": TWICE}, mode="staged")
    assert names(findings) == ["drift budget"]
    assert all(f.path == "docs/b.md" for f in findings)


def test_staged_mode_reads_the_index_not_the_working_tree(tmp_path, monkeypatch):
    git = make_repo(tmp_path, monkeypatch, BASE)
    write(tmp_path, {"docs/b.md": TWICE})
    git("add", "-A")
    write(tmp_path, {"docs/b.md": "# B\n\nPlain words only.\n"})
    findings, _ = jc.run("main", None, True)
    assert names(findings) == ["drift budget"]


def test_working_tree_mode_includes_untracked_files(tmp_path, monkeypatch):
    findings, _ = terms(tmp_path, monkeypatch, {"docs/b.md": TWICE})
    assert names(findings) == ["drift budget"]


def test_out_of_scope_files_are_ignored(tmp_path, monkeypatch):
    head = {"notes/b.md": TWICE, "docs/c.txt": TWICE}
    findings, _ = terms(tmp_path, monkeypatch, head)
    assert findings == []


def test_local_skills_are_scanned(tmp_path, monkeypatch):
    findings, _ = terms(tmp_path, monkeypatch, {".agents/skills/example/SKILL.md": TWICE})
    assert names(findings) == ["drift budget"]
    assert all(f.path == ".agents/skills/example/SKILL.md" for f in findings)


def test_deployed_skill_directory_is_ignored(tmp_path, monkeypatch):
    findings, _ = terms(tmp_path, monkeypatch, {".claude/skills/example/SKILL.md": TWICE})
    assert findings == []


def test_files_deployed_by_the_lock_are_ignored(tmp_path, monkeypatch):
    path = ".agents/skills/example/SKILL.md"
    lock = {"apm.lock.yaml": f"deployed_files:\n  - {path}\n"}
    findings, _ = terms(tmp_path, monkeypatch, {path: TWICE}, base={**BASE, **lock})
    assert findings == []
    (tmp_path / "unlocked").mkdir()
    findings, _ = terms(tmp_path / "unlocked", monkeypatch, {path: TWICE})
    assert names(findings) == ["drift budget"]


def test_exit_status(tmp_path, monkeypatch, capsys):
    make_repo(tmp_path, monkeypatch, BASE)
    assert jc.main(["--base", "main"]) == 0
    write(tmp_path, {"docs/b.md": TWICE})
    assert jc.main(["--base", "main"]) == 0
    assert 'docs/b.md:3: new-term: "drift budget" (2 uses)' in capsys.readouterr().out


SCRIPT = Path(jc.__file__)


@pytest.mark.parametrize("argv", [["--base", "nope"], ["--head", "HEAD", "--staged"], ["--bogus"]])
def test_usage_errors_exit_one(tmp_path, monkeypatch, argv):
    make_repo(tmp_path, monkeypatch, BASE)
    done = subprocess.run([sys.executable, str(SCRIPT), *argv], cwd=tmp_path, capture_output=True, text=True)
    assert done.returncode == 1


def test_environment_base_and_explicit_override(tmp_path, monkeypatch):
    git = make_repo(tmp_path, monkeypatch, BASE)
    git("branch", "stack-base")
    monkeypatch.setattr(jc, "default_base", lambda: "missing-default")
    monkeypatch.setenv("JARGON_BASE", "stack-base")
    assert jc.main([]) == 0
    monkeypatch.setenv("JARGON_BASE", "missing-ref")
    assert jc.main(["--base", "stack-base"]) == 0


@pytest.mark.parametrize("possessive", ["container's", "containers'", "container’s", "containers’"])
def test_possessive_ends_phrase(tmp_path, monkeypatch, possessive):
    text = f"The {possessive} quokka. The {possessive} quokka."
    findings, _ = terms(tmp_path, monkeypatch, {"docs/b.md": text})
    assert findings == []
    assert len(jc.segments(jc.words_of(f"{possessive} quokka", 1))) == 2


@pytest.mark.parametrize("boundary", ["plus", "except", "across", "together", "before", "inside", "outside", "between"])
def test_preposition_or_conjunction_ends_phrase(tmp_path, monkeypatch, boundary):
    text = f"Quokka {boundary} narwhal. Quokka {boundary} narwhal."
    findings, _ = terms(tmp_path, monkeypatch, {"docs/b.md": text})
    assert findings == []


@pytest.mark.parametrize("verb", ["derives", "translates", "supports", "normalizes", "proves", "skips", "installs", "parses", "resolves", "redirects", "discards", "resets", "denies"])
def test_networking_verbs_end_phrases(tmp_path, monkeypatch, verb):
    text = f"Quokka {verb} narwhal. Quokka {verb} narwhal."
    findings, _ = terms(tmp_path, monkeypatch, {"docs/b.md": text})
    assert findings == []
