# avirt

The operator CLI for avirt, built as the `avirt` binary. `avirt launch --agent claude|codex "<prompt>" [--cwd DIR] [--follow]` asks a running `avirthostd` to start an actor and can print its log until the actor ends.

```bash
cargo build -p avirt
mise run launch claude "Reply with the word pong" -- --follow
```

See [the avirt CLI](../../docs/cli.md).
