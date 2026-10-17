---
type: Component Design
title: Lifecycle and state
description: The order in which avirthostd launches and tears down an actor, avirthostd shutdown, the on-disk state root, the startup sweep after a crash, and path-length limits.
tags: [avirt, lifecycle, state, cleanup]
implementation: partial
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-17T19:00:00Z }
---

# Status: implemented

## Launch and teardown

```mermaid
sequenceDiagram
    participant CLI as avirt launch
    participant C as avirthostd
    participant S as mediator
    participant K as container CLI
    participant R as avirtinitd (root)
    participant A as adapter (uid 1000)
    CLI->>C: POST /launch
    C->>C: prepare (Codex: seed CODEX_HOME), register the actor, actor dir
    C->>S: spawn (proxy.sock, uplink.sock, hostd.sock)
    S->>C: EGRESS id secret (event connection)
    C->>C: probe both sockets until they accept
    C->>K: container run --network none --init -v proxy.sock -v uplink.sock
    C-->>CLI: actorId, logPath
    R->>S: uplink.sock (first bytes)
    S->>C: ACTOR id secret, then HTTP/2 spliced
    R->>C: POST /actors/id/stream (token), 200 headers
    R->>R: start the proxy relay, then seccomp
    R->>A: setpriv, no caps, proxy env
    A->>R: proxy relay
    R->>S: proxy.sock, CONNECT
    S->>S: private-dest check, dial upstream
    S-->>C: egress conn events
    R->>C: stream lines, exit meta, close
    K-->>C: workload exits
    C->>S: close stdin (drain, exit)
    S-->>C: events flushed, EOF
    C->>C: cleanup (Codex write-back), container_exit, remove the actor dir
```

- The mediator must be listening on both sockets before `container run`, because the runtime mounts only a socket file that exists. `avirthostd` probes both (connect and close) for up to 3 s.
- Teardown runs in the same order on every path (workload exit, kill, setup failure, avirthostd SIGINT or SIGTERM):
  1. observe the workload's exit;
  2. let the uplink drain while the mediator still splices: wait 1 s if no stream was accepted yet, then up to 10 s for an active stream to end;
  3. close the mediator's stdin and wait for it (5 s, then kill);
  4. wait for the avirthostd-side event connection to reach EOF (5 s);
  5. write the cleanup note and `container_exit`;
  6. remove the actor directory.
- If the mediator dies while the workload runs, avirthostd logs a `container_error` ("the actor mediator exited unexpectedly") and stops the workload.
- An actor's state is dropped once its exit note is written and its stream has drained. A later stream POST for an exited actor gets 410.

## avirthostd shutdown

SIGINT and SIGTERM stop avirthostd cleanly:

1. it stops every running workload;
2. it waits for each workload to exit and for its stream to drain;
3. it runs each actor's cleanup;
4. it exits.

New launches get 503 meanwhile. Cleanup also runs when setup fails partway.

## State root

Everything an actor leaves on disk lives under `$TMPDIR/avirt-<uid>/` (0700):

| Path | Holds |
| --- | --- |
| `actors/` | Each actor's private directory with its sockets |
| `scratch/` | Codex temp homes with their `auth.json` copies |
| `hostd/` | The socket mediators dial |
| `lock` | Held with an exclusive flock for avirthostd's lifetime, so a second `avirthostd` refuses to start |

## Crash cleanup

When avirthostd is killed with SIGKILL:

- its mediators exit by themselves on stdin EOF;
- its workloads keep running, with no uplink or proxy;
- its directories, its socket and any Codex temp home stay on disk.

The next `avirthostd` sweeps before it listens:

1. It stops and deletes every container labelled `avirt=1` (the label goes on every `container run`).
2. It empties `actors/`, `scratch/` and `hostd/`. Permissions are restored first, with links never followed, because the agent can `chmod 0000` its own mounted files.
3. It writes nothing back to the host, and logs what it removed.

A directory it cannot remove is logged and does not stop avirthostd. The lock and the ownership check of the state root are fatal. Mediators are not tracked by pid, since they exit on stdin EOF.

# Status: planned

## State root additions

The Containerization runtime adds these to the state root:

| Path | Holds |
| --- | --- |
| `containers/<id>/` | Each VM's runtime directory, removed at teardown and by the startup sweep |
| `cache/kernel/` | The extracted kata-containers 3.32.0 arm64 `vmlinux`, verified against its pinned sha256 |
| `cache/initfs/` | The `vminit:0.47.0` init filesystem |
| `cache/rootfs/` | Composed root filesystems, one per pair of workload image and workspace image (see [workspaces](./workspaces.md#applying-the-overlay)) |
| `images/` | avirthostd's image store with the unpacked workload and mediator images |

Every planned runtime adds `services/`, which holds the host components' listener sockets (`services/<name>.sock`, 0600) and avirthostd's issuer socket.

## Durable state

The state that must outlive the state root lives in avirthostd's durable state directory: `~/Library/Application Support/avirt/` on macOS and `$XDG_STATE_HOME/avirt/` on Linux, 0700.

| Path | Holds |
| --- | --- |
| `install-id` | The installation id that names the trust domain (see [identity](./identity.md#trust-domain-and-ids)) |
| `ca/` | The SPIFFE root key (0600) and the root certificates in the bundle |
| `peers/` | Enrolled peer trust domains and their bundles |

The startup sweep never touches the durable state directory.

## Launch order

1. For a launch with a workspace, resolve the reference through avirtregistryd and authorize `launch_workspace` for the resolved image (see [workspaces](./workspaces.md#resolution)). When another workload is the caller, apply the constraints in [workload management](./components.md#workload-management) and record its lineage. Validate the workload's alias and namespace name against the DNS label syntax and length caps, rejecting rather than truncating (see [components](./components.md#avirt-names)), assign the actor's canonical UUIDv7 and its short ID, unique on the host, regenerating on a clash (see [components](./components.md#workload-ids)), and a generated `<adverb>-<adjective>-<noun>` name unique in the namespace among live workloads, authorize the workload's creation in its namespace, each label key it carries, each credential binding it attaches (`attach_binding`) and each port it publishes (`publish_port`), against the host's Cedar snapshot, and admit the actor against the host's resource budgets. A refused launch gets 403 or 503.
2. Prepare the actor: id, the bootstrap SVIDs (workload and mediator/1), the MITM CA, the actor directory, the actor's configuration with its policy and binding references, the avirthostd channel and service channel listening, and the actor's registration with avirtsecretsd (namespace, generation, binding ids and injection points) and avirtpolicyd.
3. Run the workload (`runtime.run_workload`).
4. Wait for the runtime to ready the link endpoint. On the Containerization runtime, wait for the workload guest's relay to answer. On Docker, the link volume already exists.
5. `runtime.run_mediator`: run the mediator with `gen=1` and its channels. It has no network.
6. The link opens: avirtnetd dials with a 50 ms to 1 s backoff, because early dials fail, for up to the 30 s budget of step 7. On the Containerization runtime avirthostd splices its connection to the mediator's link end.
7. The actor is live once avirthostd accepts the uplink. With no uplink within 30 s of step 3, avirthostd tears it down.

`avirtinitd` creates `/run/avirt` itself in each container. On the Containerization runtime, avirthostd writes the workload's stdout, stderr and console to `logs/<actor-id>.stdio.ndjson` from the moment the workload starts (see [components](./components.md#workload-logs)).

## Teardown

1. The workload exits and the uplink drains.
2. `avirthostd` calls `runtime.remove` on the mediator: SIGTERM, then event flush and GOAWAY, with a 5 s limit before kill.
3. `avirthostd` waits for the events EOF and writes `container_exit`.
4. It removes the actor's generations from the fenced set at every host listener, and its registrations with avirtsecretsd and avirtpolicyd.
5. It calls `runtime.release`, which deletes both VMs or containers, and any link volume, channel socket or per-actor directory.

The mediator cannot tell a workload that is gone from one that is not ready, so avirthostd always stops it explicitly. If the mediator dies, avirthostd restarts it while restarts remain. Otherwise it stops the workload and records a `container_error`.

## Sweep

At startup, under the lock, avirthostd calls `runtime.sweep`:

1. On the Containerization runtime, it stops any VM it still owns and removes every stale `containers/<id>/` directory in the state root.
2. On Docker, it removes every container and volume labelled `avirt=1`.
3. It removes `actors/*`, including stale socket files.

The kernel, initfs and image caches in the state root are kept.

## Path length

Constraint: host Unix socket paths must be under 104 bytes. The Containerization runtime uses no host socket files. On Docker, `$TMPDIR/avirt-<uid>/actors/<id8>/c.sock` and `s.sock` come to about 90 bytes under macOS's `/var/folders/…/T/`. `avirthostd` computes the longest per-actor host socket path for its runtime at startup, and refuses to start above 100 bytes with a message suggesting a shorter `TMPDIR`.
