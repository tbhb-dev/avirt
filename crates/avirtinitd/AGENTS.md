# Agent instructions: avirtinitd

The `avirtinitd` binary, which runs only inside a Linux workload. The shared mechanics are in `avirt_core::harness`. This crate holds the two ACP conversations.

## Layout

| Module | Holds |
| --- | --- |
| `main.rs` | Dispatch on `AVIRT_AGENT` to `claude::run` or `codex::run` |
| `claude.rs` | ACP v2 (`schema::v2`, `CLAUDE_AGENT_ACP_EXPERIMENTAL_V2=1`): initialize, ACP session, prompt, permission approvals, update printing |
| `codex.rs` | ACP v1: the same steps, with session updates consumed concurrently with the prompt, and no `authenticate` call |

## Invariants

- Verify the `protocolVersion` in the initialize response and fail when it differs from the one sent.
- Permission approval goes through `harness::pick_option` (`allow_once`, else `allow_always`, else the first option), and every approval is logged as `auto_approve`.
- Never call Codex's `authenticate`, because it forces a token refresh on every launch.
- The harness owns the order: uplink accepted, then proxy relay, then seccomp, then the adapter as uid 1000 with no capabilities and `no_new_privs`. Don't spawn anything from this crate outside the harness, or it escapes that order and the filters.
- The seccomp filter (`AF_VSOCK`, `AF_NETLINK`, `AF_PACKET` and io_uring denied) exists only on Linux targets. The non-Linux build is a no-op stub, so avirtinitd is only meaningful as a Linux binary.

## Testing

There are no unit tests in this crate. `cargo test -p avirt-core` covers the harness helpers. The behavior is verified end to end: `mise run build-images`, then the regression suite in `docs/roadmap.md`, with bypass checks run inside the agent's own process tree.
