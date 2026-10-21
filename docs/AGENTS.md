---
type: Agent Instructions
title: Docs instructions
description: Scope, format and checks for repository docs.
tags: [avirt, docs, agents]
generated: { by: codex/gpt-6, at: 2026-10-21T21:36:18Z }
---

# Docs instructions

Put a doc in `docs/` when it describes built, current behavior or how to build, run, and check this repository. These are durable docs. Draft design, proposals and plans don't belong in this repository. Ask the owner about them.

Every doc needs OKF v0.2 YAML frontmatter with a non-empty `type`, plus `title`, `description`, `tags` and `generated`. The `generated` mapping names `by` and `at`. List each doc in [README.md](README.md).

Describe what the system is and does. Don't include history or decision rationale. State facts that constrain behavior as constraints. Don't coin terms. Use ordinary words or terms from the code or a standard. Write each paragraph on one line without hard wrapping. Use Mermaid for diagrams.

Run `mise run check-docs` and `guard-markdown <file>` on every Markdown file you add or edit. Run the hooks to check prose with Vale.
