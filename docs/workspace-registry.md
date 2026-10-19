---
type: Component Design
title: Workspace registry
description: avirtregistryd, with zot as a supervised child process, registry storage and repository names, the secondary index, producers, the WorkspaceRegistry search and resolve API, and the Cedar actions, trust boundaries and open questions for the registry.
tags: [avirt, workspaces, oci, registry, zot, policy]
implementation: planned
generated: { by: codex/gpt-6, at: 2026-10-18T05:20:59Z }
---

# Status: planned

This document covers avirtregistryd, which stores and indexes workspace images. The workspace image format, the config file, references and root filesystem composition are in [workspaces](./workspaces.md), and the builds that push to the registry are in [workspace builds](./workspace-build.md).

## avirtregistryd

avirtregistryd is a host service with the SPIFFE ID `spiffe://<td>/service/avirtregistryd`. It wraps **zot**, which it runs as a supervised child process, and it adds avirt's secondary index of workspace images, the search and resolve API, authorization, and every off-host registry connection.

| Item | Value |
| --- | --- |
| Registry | zot, a pinned release verified by SHA-256, with its storage under avirtregistryd's state directory (`registry/`) |
| zot extensions enabled | `search` (used internally for referrers and base and derived image queries) and `scrub` |
| zot extensions turned off | `sync`, `ui`, `userprefs`, `events`, `metrics` |
| zot listener | Loopback TCP with TLS, accepting only the client certificate that avirtregistryd issues at start from a private CA it keeps in memory. No other process can complete a handshake. |
| zot storage | The local filesystem with deduplication and garbage collection on |
| Index | A SQLite database in avirtregistryd's state directory (`registry-index.db`) |
| Off-host connections | Imports, include mirroring and exports, all through avirtedgeproxyd with credentials obtained from avirtsecretsd |
| Repository names | `<ns>/<name>` for workspaces, `mirror/<host>/<path>` for imported images and includes, `staging/<build-id>` for build outputs before promotion, `cache/<ns>/<name>` for build caches |

With `sync` turned off, zot never opens a connection of its own, including connections to another registry. Every off-host registry request goes from avirtregistryd through avirtedgeproxyd. Writes to zot pass through avirtregistryd, which updates the index in the same operation. Index reconciliation checks zot's catalog at start and on a schedule.

On a direct host install avirtregistryd starts zot before serving its own listener. Restarts use exponential backoff, and avirtregistryd exits if zot can't start. In the all-in-one server image, avirtregistryd starts in step 5 of the startup order in [components](./components.md#all-in-one-server-image), before avirtbuilderd.

### Producers

The index records each manifest's **producer** from the authenticated identity that wrote it, never from its annotations:

- `builder`: promoted by avirtbuilderd after its output checks.
- `user:<principal>`: pushed through `avirt workspace push`.
- `import`: imported from an off-host registry by `avirt workspace import` or as an include.

Resolution and policy use the producer. A user-pushed manifest's `dev.avirt.*` annotations are indexed and searchable, and their trust derives only from their producer.

### Search API

The host service listener exposes avirtregistryd's gRPC `WorkspaceRegistry` service. avirtapid forwards it for clients, and avirthostd and avirtbuilderd call it directly.

| Call | Does |
| --- | --- |
| `Search(filter, page)` | Image records matching a filter, newest first. The filter combines structured fields (repository, namespace, annotation equality and prefix, producer, base, created range) and an optional CEL expression over `image`. |
| `Resolve(reference, workload_image, fresh)` | The selected digest and ranked candidates, with reasons for each rejection or rank, plus staleness |
| `Get(digest)` | One image record with its manifest and config blob |
| `ListRepositories(namespace)`, `ListTags(repository)` | The repositories or tags the caller may pull |
| `ListRefs(repository)` | The refs avirtbuilderd watches for that repository with each ref's observed head and newest image |
| `Referrers(digest, artifact_type)` | Referrers such as signatures, SBOMs, and build provenance, from zot's referrers API |
| `Watch(filter)` | A stream of index changes |
| `UploadBuildInputs(grant, layers)` | Accepts host source layers into `staging/<build-id>`, only under the grant `WorkspaceBuilder/StartBuild` returned for that build (see [workspace builds](./workspace-build.md#builds)) |
| `Push(repository, tag, manifest, layers)` | Writes a manifest and its layers pushed by `avirt workspace push`, with producer `user:<principal>` |
| `Pull(reference)` | A stream containing a manifest with its config and layers |
| `Import(source, repository, tag)` | Imports an image from an off-host registry or from a peer (`peer:<host>/…`, see [peering](./peering.md#workspace-images)), with producer `import` |
| `Export(reference, destination)` | Pushes an image to an off-host registry |
| `Delete(digest)` | Deletion of a manifest without live workload references |

An image record contains the repository, tags, manifest digest, `artifactType`, every manifest annotation, the producer, the push time, the total and per-layer sizes, the base image reference, the provenance, each layer's origin, the build id and trigger, the referrers, the last time a launch resolved it, and the live workloads that reference it. avirtregistryd filters results by `pull_workspace` for the caller. Search reveals only what the caller may pull.

avirtregistryd doesn't serve the OCI distribution API (`/v2/`) to clients. `avirt workspace pull` and `avirt workspace push` move content through `WorkspaceRegistry` calls that avirtapid forwards.

## Policy

| Action | Evaluated by | Resource | Context |
| --- | --- | --- | --- |
| `push_workspace` | avirtregistryd, per manifest write | `WorkspaceRepository` | `via`, `producer` |
| `pull_workspace` | avirtregistryd, per read, search result, and resolve candidate | `WorkspaceImage` | `via`, `purpose` (`client`, `launch`, `build-input`, `include` or `peer-import`) |
| `import_workspace`, `export_workspace` | avirtregistryd | `WorkspaceRepository` | `via`, `registry` (the off-host registry host, or `peer:<host>` for a peer), `reference`, and `allow_local` for an export |
| `delete_workspace` | avirtregistryd | `WorkspaceImage` | `via` |

## Trust boundaries

| Listener | Where | Transport | Caller identity | Allowed operations |
| --- | --- | --- | --- | --- |
| avirtregistryd (`services/avirtregistryd.sock`) | Host, avirtregistryd | Host service listener | avirtapid with `x-avirt-caller`, avirtbuilderd, avirthostd, and avirtmediatord processes for live build jobs at their current generations | `WorkspaceRegistry` calls for avirtapid, avirtbuilderd and avirthostd, each authorized by Cedar. For a build job, the OCI distribution API on its own grant only. |
| zot (`127.0.0.1:<port>`) | Host, zot | Loopback TCP, mutual TLS with a private CA | avirtregistryd only | The OCI distribution API and zot's search extension |

| Compromised | Can reach | Can't reach |
| --- | --- | --- |
| avirtregistryd | Every stored workspace image, and the registry credentials it obtains from avirtsecretsd. Serving a forged index or forged content to avirthostd. | Running content. avirthostd verifies every pulled digest against the resolved manifest. Forged content fails at launch, while `launch_workspace` on the returned image record bounds a forged resolution |

# Open questions

1. **Who can pull a local image.** Being built from local sources keeps an image out of selector resolution and exports. Whether local images restrict pulls to their builder principal remains open.
