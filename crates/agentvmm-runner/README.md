# agentvmm-runner

The workload entrypoint for agentvmm-poc, built as the `agentvmm-runner` binary. It is a Linux guest binary only, built into each workload image. It runs as root, opens the actor uplink, relays the agent's proxy traffic, and starts the ACP adapter as uid 1000 under seccomp. `AGENTVMM_AGENT=claude|codex`, set in each image, selects the agent.

```bash
mise run build-images
```

See [agents](../../docs/agents.md) and [workload hardening](../../docs/workload-hardening.md).
