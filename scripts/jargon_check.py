#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["markdown-it-py==4.0.0"]
#
# [tool.uv]
# exclude-newer = "2026-10-01T00:00:00Z"
# ///
"""Flag invented jargon: terms an added line of prose introduces that nobody defined.

Usage: uv run --script scripts/jargon_check.py [--base REF] [--head REF | --staged]

The check reads only the lines a change adds to in-scope Markdown (docs/, root and crate
Markdown, local skills) and parses the whole post-change file so code, links, frontmatter and
HTML never count as prose. It flags a hyphenated compound or a 2 to 4 word phrase that the base
ref's prose and code never use and that nothing allows. Without --head it reads the working tree;
--staged reads the index. The base is compared at its merge base with the head.

A term is allowed by an entry heading in docs/glossary.md, a line of
.vale/config/vocabularies/project/accept.txt, or a line of jargon-accept.txt with a `# reason`.
Candidates are advisory for human review. The report exits 0 even when it finds candidates.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import re
import subprocess
import sys
from collections import Counter, defaultdict
from dataclasses import dataclass
from pathlib import Path

from markdown_it import MarkdownIt



def load_words():
    """Load the word lists beside this script, whatever the working directory."""
    path = Path(__file__).resolve().with_name("jargon_words.py")
    spec = importlib.util.spec_from_file_location("jargon_words", path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_words = load_words()
COMMON_COMPOUNDS = _words.COMMON_COMPOUNDS
COMPOUND_PREFIXES = _words.COMPOUND_PREFIXES
STOPWORDS = _words.STOPWORDS
VERB_STEMS = _words.VERB_STEMS

INDEX_VERSION = 2
ALLOW_FILE = "jargon-accept.txt"
ACCEPT_FILE = ".vale/config/vocabularies/project/accept.txt"
GLOSSARY = "docs/glossary.md"
LOCK_FILE = "apm.lock.yaml"
CODE_SUFFIXES = (".rs", ".toml", ".yml", ".yaml", ".json", ".sh", ".py")
MAX_CODE_BYTES = 400_000
MAX_PHRASE = 4
MIN_PHRASE_USES = 2

BOUNDARY = "|"
WORD = re.compile(r"[a-z]+(?:'[a-z]+)?(?:-[a-z]+)*")
IDENT_PART = re.compile(r"[A-Z]+(?![a-z])|[A-Z]?[a-z]+")
COMPOUND = re.compile(r"[A-Za-z]+(?:-[A-Za-z]+)+")
# A whitespace-delimited chunk that is a URL, path, version, hash, number or identifier.
NON_PROSE_CHUNK = re.compile(
    r"""(?x)
    \S*://\S* | \S*[/\\@$%#=<>~*_^`\[\]{}]\S* | \S*\d\S* | \S*::\S* | \S*[A-Za-z]\.[A-Za-z]\S* | --\S*
    """
)
INLINE_PARSER = MarkdownIt("commonmark").enable("table")


# ---------------------------------------------------------------------------------------------
# Normalization


def fold(word: str) -> str:
    """Lowercase a word, drop its possessive ending and fold a plural to its singular."""
    word = word.lower().replace("’", "'")
    if word.endswith("'s"):
        word = word[:-2]
    word = word.rstrip("'")
    if len(word) > 4 and word.endswith("ies"):
        return word[:-3] + "y"
    if len(word) > 4 and re.search(r"(?:ss|x|z|ch|sh)es$", word):
        return word[:-2]
    if len(word) > 3 and word.endswith("s") and not word.endswith(("ss", "us", "is")):
        word = word[:-1]
    # A singular in -che, -she, -ze, -xe or -sse shares a key with its plural, which lost the e.
    if len(word) > 3 and re.search(r"(?:ch|sh|ss|x|z)e$", word):
        return word[:-1]
    return word


COMMON_WORDS = frozenset(fold(w) for w in _words.COMMON_WORDS)


def norm_term(text: str) -> str:
    """Normal form of a phrase: folded words joined by single spaces, hyphens kept."""
    return " ".join(fold(w) for w in re.findall(r"[A-Za-z][A-Za-z'’-]*", text))


def split_identifier(text: str) -> list[str]:
    """Split snake_case, kebab-case and camelCase text into lowercase words."""
    words = []
    for chunk in re.split(r"[^A-Za-z]+", text):
        words.extend(p.lower() for p in IDENT_PART.findall(chunk))
    return words


# ---------------------------------------------------------------------------------------------
# Markdown prose extraction


@dataclass(frozen=True)
class Word:
    text: str  # normalized (folded) word, hyphens kept
    raw: str  # lowercase surface form
    line: int  # 1-based source line


def blank_frontmatter(text: str) -> str:
    """Replace a leading `---` frontmatter block with blank lines so line numbers hold."""
    lines = text.split("\n")
    if lines and lines[0].strip() == "---":
        for i in range(1, len(lines)):
            if lines[i].strip() == "---":
                return "\n".join([""] * (i + 1) + lines[i + 1 :])
    return text


def words_of(text: str, line: int) -> list[Word | None]:
    """Tokenize a prose run into words, with None marking a phrase boundary."""
    out: list[Word | None] = []
    for chunk in text.replace("’", "'").split():
        if NON_PROSE_CHUNK.fullmatch(chunk.strip(".,;:!?()\"'")) or NON_PROSE_CHUNK.fullmatch(chunk):
            out.append(None)
            continue
        # Punctuation other than a hyphen or apostrophe ends a phrase.
        for piece in re.split(r"([^A-Za-z'-]+)", chunk):
            if not piece:
                continue
            if not re.fullmatch(r"[A-Za-z'-]+", piece):
                out.append(None)
                continue
            possessive = bool(re.search(r"(?:'s|s')$", piece, re.I))
            raw = piece.strip("-'").lower()
            if piece.lower().startswith("'s") and raw == "s":
                continue  # a possessive left over after inline code: `foo`'s
            if len(raw) < 2 or not re.search(r"[a-z]", raw):  # also a unit left by "30 s"
                out.append(None)
                continue
            raw = re.sub(r"-{2,}", "-", raw)
            out.append(Word("-".join(fold(p) for p in raw.split("-")), raw, line))
            if possessive:
                out.append(None)
    return out


def prose_words(text: str, added: set[int] | None = None) -> list[Word | None]:
    """Parse Markdown and return its prose words; with `added`, only those on added lines.

    Skipped: fenced and indented code, inline code, HTML, link targets, image alt text,
    frontmatter, and non-prose chunks (URLs, paths, versions, hashes, numbers with units).
    A skipped region leaves a boundary so no phrase spans it.
    """
    out: list[Word | None] = []
    for tok in INLINE_PARSER.parse(blank_frontmatter(text)):
        if tok.type != "inline" or not tok.map:
            if tok.type.endswith("_open") or tok.type.endswith("_close") or tok.type in ("fence", "code_block", "html_block"):
                out.append(None)
            continue
        line = tok.map[0] + 1
        for child in tok.children or []:
            if child.type == "text":
                for w in words_of(child.content, line):
                    out.append(w if w is None or added is None or w.line in added else None)
            elif child.type in ("softbreak", "hardbreak"):
                line += 1
                out.append(None)
            elif child.type in ("code_inline", "html_inline", "image"):
                out.append(None)
        out.append(None)
    return out


def segments(words: list[Word | None]) -> list[list[Word]]:
    groups: list[list[Word]] = [[]]
    for w in words:
        if w is None:
            if groups[-1]:
                groups.append([])
        else:
            groups[-1].append(w)
    return [s for s in groups if s]


def code_text(text: str) -> str:
    """The text of every code region (inline, fenced, indented) of a Markdown file."""
    parts = []
    for tok in INLINE_PARSER.parse(blank_frontmatter(text)):
        if tok.type in ("fence", "code_block"):
            parts.append(tok.content)
        for child in tok.children or []:
            if child.type == "code_inline":
                parts.append(child.content)
    return "\n".join(parts)


# ---------------------------------------------------------------------------------------------
# Git access


def git(*args: str, check: bool = True) -> str:
    proc = subprocess.run(["git", *args], capture_output=True, text=True, check=False)
    if check and proc.returncode:
        raise SystemExit(f"jargon-check: git {' '.join(args)}: {proc.stderr.strip()}")
    return proc.stdout


def ref_exists(ref: str) -> bool:
    return subprocess.run(["git", "rev-parse", "--verify", "-q", ref + "^{commit}"], capture_output=True).returncode == 0


def default_base() -> str:
    return "origin/main" if ref_exists("origin/main") else "main"


def read_blob(ref: str | None, path: str, staged: bool = False) -> str | None:
    """File text at a ref, in the index (staged), or in the working tree (ref None)."""
    if ref is None and not staged:
        p = Path(git("rev-parse", "--show-toplevel").strip()) / path
        return p.read_text(encoding="utf-8", errors="replace") if p.is_file() else None
    spec = f":{path}" if staged else f"{ref}:{path}"
    proc = subprocess.run(["git", "show", spec], capture_output=True, check=False)
    return proc.stdout.decode("utf-8", errors="replace") if proc.returncode == 0 else None


def deployed_paths(lock_text: str | None) -> set[str]:
    return set(re.findall(r"^\s*-\s+(\.\S+)\s*$", lock_text or "", re.M))


def in_scope(path: str, deployed: set[str]) -> bool:
    if not path.endswith(".md") or path in deployed:
        return False
    parts = path.split("/")
    if parts[0] == "docs" or parts[0] == "crates":
        return True
    if len(parts) == 1:
        return True
    return parts[:2] == [".agents", "skills"]


def tree_files(ref: str) -> list[str]:
    return [p for p in git("ls-tree", "-r", "--name-only", ref).split("\n") if p]


# ---------------------------------------------------------------------------------------------
# Repo index


@dataclass
class Index:
    phrases: set[str]  # folded 1 to 4 word phrases of base prose and split code identifiers
    compounds: set[str]  # folded hyphenated compounds of base prose and code

    def has(self, term: str) -> bool:
        return term in self.phrases or term in self.compounds


def add_ngrams(target: set[str], words: list[str]) -> None:
    for i in range(len(words)):
        for n in range(1, MAX_PHRASE + 1):
            if i + n <= len(words):
                target.add(" ".join(words[i : i + n]))


def build_index(ref: str) -> Index:
    phrases: set[str] = set()
    compounds: set[str] = set()
    deployed = deployed_paths(read_blob(ref, LOCK_FILE))
    for path in tree_files(ref):
        text = None
        if in_scope(path, deployed):
            text = read_blob(ref, path)
            for seg in segments(prose_words(text or "")):
                add_ngrams(phrases, [w.text for w in seg])
                compounds.update(w.text for w in seg if "-" in w.text)
            text = code_text(text or "")
        elif path.endswith(CODE_SUFFIXES) and not path.startswith((".vale/", ".devtools/", "apm_modules/")):
            text = read_blob(ref, path)
            if text is not None and len(text) > MAX_CODE_BYTES:
                text = None
        if not text:
            continue
        compounds.update(fold_compound(m) for m in COMPOUND.findall(text))
        for line in text.split("\n"):
            idents = re.findall(r"[A-Za-z][A-Za-z0-9_]*", line)
            words = [fold(p) for ident in idents for p in split_identifier(ident)]
            add_ngrams(phrases, words)
    return Index(phrases, compounds)


def fold_compound(text: str) -> str:
    return "-".join(fold(p) for p in text.split("-"))


def load_index(base_commit: str) -> Index:
    root = os.environ.get("JARGON_CACHE_DIR") or str(Path(git("rev-parse", "--show-toplevel").strip()) / "target" / "jargon")
    cache = Path(root) / f"{base_commit}.json"
    try:
        data = json.loads(cache.read_text())
        if data.get("version") == INDEX_VERSION:
            return Index(set(data["phrases"]), set(data["compounds"]))
    except (OSError, ValueError, KeyError):
        pass
    index = build_index(base_commit)
    try:
        cache.parent.mkdir(parents=True, exist_ok=True)
        cache.write_text(json.dumps({"version": INDEX_VERSION, "phrases": sorted(index.phrases), "compounds": sorted(index.compounds)}))
    except OSError:
        pass
    return index


# ---------------------------------------------------------------------------------------------
# Allowed terms


@dataclass
class Allowed:
    terms: set[str]
    patterns: list[re.Pattern[str]]

    def has(self, term: str, raw: str) -> bool:
        return term in self.terms or any(p.fullmatch(raw) for p in self.patterns)


def glossary_terms(text: str | None) -> list[str]:
    return [norm_term(m) for m in re.findall(r"^#{2,6}\s+(.+?)\s*$", re.sub(r"<!--.*?-->", "", text or "", flags=re.S), re.M)]


def allow_entries(text: str | None) -> tuple[dict[str, int], list[tuple[int, str]]]:
    """Parse jargon-accept.txt into ({term: line}, [(line, term)] of entries with no reason)."""
    good: dict[str, int] = {}
    bad: list[tuple[int, str]] = []
    for n, line in enumerate((text or "").split("\n"), 1):
        body, _, reason = line.partition("#")
        term = norm_term(body)
        if not term:
            continue
        if reason.strip():
            good[term] = n
        else:
            bad.append((n, term))
    return good, bad


def accept_patterns(text: str | None) -> list[re.Pattern[str]]:
    out = []
    for line in (text or "").split("\n"):
        line = line.strip()
        if line and not line.startswith("#"):
            try:
                out.append(re.compile(line, re.I))
            except re.error:
                continue
    return out


# ---------------------------------------------------------------------------------------------
# Detection


def is_common(word: str) -> bool:
    return all(fold(p) in COMMON_WORDS or p in COMMON_WORDS for p in word.split("-"))


def verb_like(word: str) -> bool:
    """True for a verb stem in any inflection, an adverb, or a participle."""
    for part in word.split("-"):
        stems = {part, part.rstrip("e"), part[:-1], part[:-2], part[:-3], part[:-3] + "e", part[:-2] + "y"}
        if part.endswith(("ed", "ly")) and len(part) > 4 or part.endswith("ing") and len(part) > 5:
            return True
        if part in VERB_STEMS or (part.endswith(("s", "ed", "ing", "d")) and stems & VERB_STEMS):
            return True
    return False


def noun_phrase(words: list[str], raw: list[str]) -> bool:
    """False for a sentence fragment: it holds a verb, or a component name followed by one."""
    if any(verb_like(w) for w in [*words, *raw]):
        return False
    # A component name takes a verb next, not a noun: "avirtmediatord injects".
    return not any(a.startswith("avirt") and "'" not in a and b.endswith("s") for a, b in zip(raw, raw[1:]))


def is_stop(word: str) -> bool:
    return word in STOPWORDS or word.replace("'", "") in STOPWORDS


@dataclass
class Finding:
    path: str
    line: int
    term: str
    uses: int
    kind: str = "new-term"

    def render(self) -> str:
        if self.kind != "new-term":
            return f'{self.path}:{self.line}: {self.kind}: "{self.term}"'
        return f'{self.path}:{self.line}: new-term: "{self.term}" ({self.uses} {"use" if self.uses == 1 else "uses"})'


def ordinary_compound(term: str, index: Index) -> bool:
    parts = term.split("-")
    if term in COMMON_COMPOUNDS or all(is_common(p) for p in parts):
        return True
    # A prefixed word whose stem the repo already uses: "non-blocking" when "blocking" occurs.
    return parts[0] in COMPOUND_PREFIXES and all(p in index.phrases or is_common(p) for p in parts[1:])


def known_variant(term: str, index: Index) -> bool:
    """A compound the repo already writes as separate words or as one word."""
    parts = term.split("-")
    return " ".join(parts) in index.phrases or "".join(parts) in index.phrases


def find_terms(
    files: dict[str, tuple[str, set[int]]], index: Index, allowed: Allowed
) -> list[Finding]:
    """Flag new compounds and repeated new phrases across every file's added prose."""
    first: dict[tuple[str, str], tuple[int, str]] = {}  # (path, term) -> (first line, raw form)
    uses: Counter[tuple[str, str]] = Counter()
    for path, (text, added) in files.items():
        for seg in segments(prose_words(text, added)):
            for i, w in enumerate(seg):
                if "-" in w.text:
                    note(first, uses, path, w.text, w.line, w.raw)
                for n in range(2, MAX_PHRASE + 1):
                    if i + n <= len(seg):
                        gram = seg[i : i + n]
                        term = " ".join(g.text for g in gram)
                        note(first, uses, path, term, gram[0].line, " ".join(g.raw for g in gram))

    raw_form = {t: r for (_, t), (_, r) in first.items()}
    total: Counter[str] = Counter()
    for (_, term), n in uses.items():
        total[term] += n

    flagged: dict[str, int] = {}
    for term, n in total.items():
        words = term.split(" ")
        if " " not in term:
            skip = ordinary_compound(term, index) or known_variant(term, index)
        else:
            skip = (
                n < MIN_PHRASE_USES
                or any(is_stop(w) for w in words)
                or all(is_common(w) for w in words)
                or not noun_phrase(words, raw_form[term].split(" "))
            )
        if skip or index.has(term):
            continue
        raw = raw_form[term]
        if allowed.has(term, raw) or (" " in term and allowed.has(term.replace(" ", "-"), raw)):
            continue
        flagged[term] = n

    # Keep the longest phrase when it holds a shorter flagged one that never occurs on its own.
    kept = {
        t: n
        for t, n in flagged.items()
        if not any(t != u and contains(u, t) and flagged[u] == n for u in flagged)
    }
    findings = []
    for (path, term), n in uses.items():
        if term in kept:
            line, raw = first[(path, term)]
            findings.append(Finding(path, line, raw, n))
    return findings


def contains(longer: str, shorter: str) -> bool:
    return f" {shorter} " in f" {longer} " and len(longer) > len(shorter)


def note(first, uses, path: str, term: str, line: int, raw: str) -> None:
    key = (path, term)
    first.setdefault(key, (line, raw))
    uses[key] += 1


# ---------------------------------------------------------------------------------------------
# Diff handling


def added_lines(base: str, head: str | None, staged: bool) -> dict[str, set[int]]:
    cmd = ["diff", "-U0", "--no-color", "--no-renames", "--diff-filter=AM"]
    cmd += ["--cached", base] if staged else [base] if head is None else [base, head]
    out: dict[str, set[int]] = defaultdict(set)
    path = None
    for line in git(*cmd, "--", "*.md", ALLOW_FILE, ACCEPT_FILE).split("\n"):
        if line.startswith("+++ "):
            path = line[6:] if line.startswith("+++ b/") else None
        elif line.startswith("@@") and path:
            m = re.match(r"@@ -\S+ \+(\d+)(?:,(\d+))? @@", line)
            start, count = int(m.group(1)), int(m.group(2) if m.group(2) is not None else 1)
            out[path].update(range(start, start + count))
    if head is None and not staged:  # untracked Markdown is wholly added
        for path in git("ls-files", "--others", "--exclude-standard", "--", "*.md").split("\n"):
            text = read_blob(None, path) if path else None
            if text is not None:
                out[path].update(range(1, text.count("\n") + 2))
    return out


def run(base: str, head: str | None, staged: bool) -> tuple[list[Finding], list[str]]:
    target = head or "HEAD"
    base_commit = git("merge-base", base, target).strip()
    changed = added_lines(base_commit, head, staged)
    blob_ref, from_index = (head, False) if head else (None, staged)
    deployed = deployed_paths(read_blob(blob_ref, LOCK_FILE, from_index))

    files = {}
    for path, lines in changed.items():
        if in_scope(path, deployed):
            text = read_blob(blob_ref, path, from_index)
            if text is not None:
                files[path] = (text, lines)

    gloss_now = glossary_terms(read_blob(blob_ref, GLOSSARY, from_index))
    gloss_base = glossary_terms(read_blob(base_commit, GLOSSARY))
    allow_now, missing = allow_entries(read_blob(blob_ref, ALLOW_FILE, from_index))
    allow_base, _ = allow_entries(read_blob(base_commit, ALLOW_FILE))
    accept_now = accept_patterns(read_blob(blob_ref, ACCEPT_FILE, from_index))
    allowed = Allowed(set(gloss_now) | set(allow_now), accept_now)

    findings = find_terms(files, load_index(base_commit), allowed) if files else []
    for n, term in missing:
        findings.append(Finding(ALLOW_FILE, n, term, 0, kind="missing-reason"))

    summary = []
    new_allow = sorted(set(allow_now) - set(allow_base))
    new_gloss = sorted(set(gloss_now) - set(gloss_base))
    old_accept = {l.strip() for l in (read_blob(base_commit, ACCEPT_FILE) or "").split("\n")}
    new_accept = sorted({l.strip() for l in (read_blob(blob_ref, ACCEPT_FILE, from_index) or "").split("\n") if l.strip()} - old_accept)
    summary.append(f"allow entries added: {len(new_allow)}" + "".join(f"\n  {ALLOW_FILE}: {t}" for t in new_allow))
    summary.append(f"glossary entries added: {len(new_gloss)}" + "".join(f"\n  {GLOSSARY}: {t}" for t in new_gloss))
    summary.append(f"accept.txt entries added: {len(new_accept)}" + "".join(f"\n  accept.txt: {t}" for t in new_accept))
    return findings, summary


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--base", help="base ref; default JARGON_BASE, then origin/main, or main without a remote ref")
    where = parser.add_mutually_exclusive_group()
    where.add_argument("--head", help="head ref; default is the working tree")
    where.add_argument("--staged", action="store_true", help="read the staged index")
    try:
        args = parser.parse_args(argv)
    except SystemExit as exc:
        return 1 if exc.code else 0

    findings, summary = run(args.base or os.environ.get("JARGON_BASE") or default_base(), args.head, args.staged)
    for f in sorted(findings, key=lambda f: (f.path, f.line, f.term)):
        print(f.render())
    print("\n".join(summary), file=sys.stderr)
    if findings:
        print(f"jargon-check: {len(findings)} candidate(s) for human review; this report does not block checks.", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
