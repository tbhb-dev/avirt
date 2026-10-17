# agentvmm-cli

The operator CLI for agentvmm-poc, built as the `agentvmm` binary. `agentvmm launch --agent claude|codex "<prompt>" [--cwd DIR] [--follow]` asks a running controller to start an actor and can print its log until the actor ends.

```bash
cargo build -p agentvmm-cli
mise run launch claude "Reply with the word pong" -- --follow
```

See [the agentvmm CLI](../../docs/cli.md).
