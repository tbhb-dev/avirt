---
type: Component Design
title: Workspace builds
description: avirtbuilderd and workspace builds, with the three build kinds, sandboxed build jobs, the build pipeline and promotion, caching, build stages with hooks and the Containerfile, immutable and mutable tags, prebuild policies with their triggers and retention, and the Cedar actions, trust boundaries and open questions for builds.
tags: [avirt, workspaces, builder, buildkit, cel, policy]
implementation: planned
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-17T22:30:00Z }
---
# Status: planned

This document covers how avirtbuilderd builds workspace images. The workspace image format, the config file, references and root filesystem composition are in [workspaces](./workspaces.md), and the registry builds push to is in [workspace registry](./workspace-registry.md).

## Builds

avirtbuilderd owns the build queue, build records, watch registrations and the ref heads it observes. It holds no SVID beyond its own `/service/avirtbuilderd`, no signing keys, and no host control API access. It never parses source content or executes build commands itself.

avirtbuilderd serves a gRPC `WorkspaceBuilder` service on its host service listener, which avirtapid forwards for clients: `StartBuild`, `ListBuilds`, `GetBuild`, `ReadBuildLog`, `CancelBuild`, `AddPrebuildPolicy`, `ListPrebuildPolicies` and `RemovePrebuildPolicy`. The commands that call them are in [operator CLI](./operator-cli.md#workspaces).

### Build kinds

| Kind | When | Where it runs |
| --- | --- | --- |
| Assembly | Every layer is produced by the CLI from host sources, with no includes, no hooks and no Containerfile | avirtbuilderd, without a build job: it checks the CLI's layers and assembles the manifest |
| Fetch | There is a remote git or github source or an include, and no hooks or Containerfile | A build job that runs only `git` and avirt's layer tool |
| Command | There are hooks or a Containerfile | A build job that also runs BuildKit |

### Build jobs

A build job is a workload that avirthostd creates for avirtbuilderd: a workload container and its mediator, created like any workload but with the build security context. It is in the workspace's namespace and carries the reserved labels `avirt.internal/build` (the build id) and `avirt.internal/workspace` (the repository), so policy can tell it apart from agent workloads.

- **Image.** The workload container runs the avirt builder image, which holds avirtinitd, avirtnetd, avirtexec in build mode, `git`, avirt's layer tool and BuildKit. The build's base image and the input layers are inputs to the build, never the job's own root filesystem.
- **Identity.** The job's only SVID is its workload SVID in avirtnetd. It has no connection to avirthostd of its own beyond its mediator's, and no access to avirtapid: avirthostd refuses any `guest-cli` stream for a build job.
- **Inputs.** avirtnetd serves a registry socket to the build runner, and avirtmedproxyd forwards it to avirtregistryd through avirthostd's SNI routing under the job's identity. avirtregistryd grants the job read access to exactly the base image, the source bundle and the include digests of its build, read access to the cache scopes in [caching](#caching), and write access to `staging/<build-id>` only.
- **Network.** Every other connection goes through avirtnetd's forward proxy, the job's avirtmedproxyd and avirtedgeproxyd, under the job's workload policy snapshot. The build policy for the job permits the sources' hosts for the fetch and the package registries the host policy names for commands.
- **Credentials.** A private source's credential, such as a short-lived GitHub installation token, is a mediator-injected credential binding for the source's host, so the job never holds the real credential. Only the fetch step's connections are intercepted for injection. Connections from build commands are not intercepted and carry no injected credential, and the commands' environment trusts only the base image's CA bundle.
- **Security context.** avirtinitd runs the build runner as root inside the build VM with the build security context, which permits the namespaces and mounts BuildKit's OCI worker needs, and applies no Landlock ruleset to it (see [build jobs in sandbox](./sandbox.md#build-jobs)). Build commands run in BuildKit containers with the VM's loopback network, so they reach only avirtnetd's proxy.
- **Limits.** avirthostd admits the job against the host's resource budgets like any workload, with a wall-clock deadline, and tears it down when it ends. Nothing from the job persists except what it pushed to `staging/<build-id>` and its logs.
- **Logs.** The job's workload log and stdio log are written like any workload's, and `avirt workspace builds logs` reads them through avirtapid.

Command builds require a runtime with `isolation = VmPerWorkload`. On Docker Desktop, where every workload container shares one kernel, avirtbuilderd runs assembly and fetch builds and refuses command builds with `build requires vm isolation` (see [compute runtimes](./compute-runtimes.md#docker-desktop-runtime)).

### Build pipeline

```mermaid
sequenceDiagram
    participant CLI as avirt
    participant P as avirtapid
    participant B as avirtbuilderd
    participant R as avirtregistryd
    participant H as avirthostd
    participant J as build job
    participant F as avirtedgeproxyd
    CLI->>CLI: read the config, package host sources as layers
    CLI->>P: StartBuild
    P->>B: StartBuild, as the caller
    B->>B: authorize build_workspace, plan the build
    B-->>CLI: build id and staging grant
    CLI->>P: UploadBuildInputs (source bundle)
    P->>R: host source layers into staging, under the grant
    B->>R: import includes and the base if missing
    R->>F: off-host registries
    B->>H: create a build job (fetch or command)
    J->>R: read inputs (registry socket, SNI routing)
    J->>F: fetch sources, install dependencies (avirtmedproxyd)
    J->>R: push the result to staging/BUILD-ID
    H-->>B: job exited
    B->>R: read the staged manifest and layers
    B->>B: check digests, base image reference and content rules
    B->>R: promote to NS/NAME with annotations and tags
    B-->>P: build record
    P-->>CLI: digest and tags
```

Promotion is the only way a build result reaches a workspace repository. avirtbuilderd writes the promoted manifest itself, with annotations it computes from what it asked to be built, so nothing the job writes reaches an annotation, a tag or the index unchecked.

### Caching

BuildKit in a command build imports and exports its cache through the registry socket, to `cache/<ns>/<name>`, in scopes keyed by ref. A build of a ref reads the scope of its own ref and of the repository's default branch, and writes only the scope of its own ref. A pull request build therefore never writes a cache that a default branch build reads.

## Build stages and the Containerfile

A build runs its steps in this order, and each step that runs commands adds layers above the previous one:

1. the build's base image (`build.base`);
2. the `pre-source` hook commands, for system packages;
3. the source layers, then the include layers, of the `dependencies` stage, in config order;
4. the `dependencies` hook commands, for installing dependencies;
5. the source layers, then the include layers, of the `source` stage, in config order;
6. the `post-source` hook commands;
7. the Containerfile, when it exists.

The steps are ordered by how often their inputs change. A `dependencies` stage source takes only manifests and lockfiles, so step 4 stays cached until one of them changes, and an edit to the source tree reruns only steps 5 to 7. The dependency layers also sit below the source layers in the image, so a host keeps them across source edits.

The workspace image is every layer above the build's base image. A build with no hooks and no Containerfile has base image reference `none`.

The Containerfile's first stage is `FROM workspace`, where `workspace` names the result of steps 1 to 6, and it can also use `dependencies`, the result of steps 1 to 4. It may have further stages, and its final stage must descend from `workspace`. avirtbuilderd translates the hooks into stages of the same build, so hooks and a Containerfile together form one build.

```dockerfile
FROM workspace
WORKDIR /workspace
RUN npm ci && npm run build
```

## Tags

A tag points at one manifest at a time, and pushing another manifest under the same tag moves it. Every promoted manifest therefore gets an **immutable tag**, `build-<build id>` (build ids are ULIDs, so the tag is always valid), that no other build reuses and only retention or `avirt workspace rm` removes. Without it, a manifest whose mutable tag moved on would be left untagged, and registries commonly garbage-collect untagged manifests, as most off-host registries' retention rules do. The immutable tags also keep avirtregistryd's index rebuildable: listing a repository's tags and reading each manifest recovers every image record.

A build can also carry **mutable tags**: `--tag` for an on-demand build, and the rendered `tag-format` for a prebuild. Mutable tags move, and `github:` and `git:` selectors never read them; they select by annotations through the index. A mutable tag is made valid as an OCI tag: every character outside `[A-Za-z0-9_.-]` becomes `-`, a leading `.` or `-` becomes `_`, and a tag longer than 128 characters keeps its first 115 characters followed by `-` and the first 12 hex characters of the SHA-256 of the untruncated tag. `dev.avirt.workspace.tag-key` records the mutable tag before sanitization. Every build is also addressable by its manifest digest.

## Prebuild policies

A **prebuild policy** tells avirtbuilderd which refs of one repository to keep built. It is a TOML file that an operator registers with `avirt workspace prebuild add`, never a file in the repository it builds: a repository's own files cannot choose which of its refs get built, where the results are promoted, or how long they are kept.

```toml
name = "bar-previews"
repository = "github:foo/bar"         # or "git:<url>"
workspace = "team-a/bar"              # the workspace repository builds are promoted to
refs = 'ref.default || ref.kind == "pull" || (ref.kind == "branch" && ref.short.startsWith("feat/"))'
tag-format = "{{ ref.short }}"
config = "tree"                       # default; or a path whose content is registered with the policy
lock = "tree"                         # default; or "refresh"

[triggers]
schedule = "0 */6 * * *"
events = ["push", "pull_request.opened", "pull_request.synchronize", "pull_request.closed"]
poll-interval = "60s"

[retention]
keep-per-ref = 3
closed-pull-requests = "7d"
```

| Key | Default | Meaning |
| --- | --- | --- |
| `name` | Required | The policy's name, unique on the host |
| `repository` | Required | A `github:` or `git:` reference with no selector |
| `workspace` | Required | The workspace repository builds are promoted to. The built config's `name` and `namespace` are ignored. |
| `refs` | `ref.default` | A CEL expression over each ref (see [expressions](./workspaces.md#expressions)). avirtbuilderd keeps an image built for every ref it accepts. |
| `tag-format` | None | A template for each build's mutable tag (see [tags](#tags)) |
| `config` | `tree` | `tree` reads `.avirt-workspace.toml` from each built commit, with the zero-config defaults when there is none. A path registers that file's content with the policy and uses it for every ref. |
| `lock` | `tree` | `tree` uses the built commit's lockfile. `refresh` resolves every entry fresh. |
| `triggers.schedule`, `triggers.events`, `triggers.poll-interval` | None, none, `60s` | What starts builds, below |
| `retention.keep-per-ref`, `retention.closed-pull-requests` | `3`, `7d` | Which of the policy's images avirtregistryd keeps |

- **What a prebuild builds.** For each accepted ref, avirtbuilderd builds the workspace config at the ref's commit, with the primary source fixed to the policy's repository at that commit. A config whose primary source is another repository, or that names a source read on the host, fails that ref's build with `config not buildable by policy`. Each build is authorized with `build_workspace` like any other, so a pull request that edits the config to reach new hosts or owners is still bounded by policy, and its hooks and Containerfile run in a build job like any command build.
- **Schedule.** `triggers.schedule` is a five-field cron expression in the host's time zone. Each run rebuilds every accepted ref whose newest image is older than the schedule interval, which refreshes images whose dependencies moved while their sources did not.
- **Events.** avirtbuilderd polls the repository at `triggers.poll-interval` with conditional requests, and derives events from the ref heads it observes. `push` is a branch or tag head that moved. `pull_request.opened` and `pull_request.synchronize` are a new or moved `refs/pull/<n>/head`. `pull_request.closed` is a pull request that closed, and it builds the base branch when the pull request merged. An event builds only refs that `refs` accepts.
- **Retention.** avirtregistryd keeps the `keep-per-ref` newest images per ref that the policy produced, deletes the images of a closed pull request once `closed-pull-requests` has passed, and never deletes an image that a live workload's configuration references. Deleting an image removes its immutable tag with it.

## Policy

| Action | Evaluated by | Resource | Context |
| --- | --- | --- | --- |
| `build_workspace` | avirtbuilderd, per build | `WorkspaceRepository` | `via`, `kind` (`assembly`, `fetch` or `command`), `source_types`, `source_hosts` and `github_owners` (sets of strings), `includes` (a set of image references), `provenance`, `config_source`, `secret_allows` (a set of patterns), `trigger`, `ref`, `owner`, and `prebuild_policy` for a prebuild |
| `watch_workspace` | avirtbuilderd, per prebuild policy registration | `WorkspaceRepository` (the policy's `workspace`) | `via`, `repository` (the watched remote), `config` (`tree` or `registered`), `events`, `schedule` |

A build job's network access is governed by the existing `resolve`, `connect` and `request` actions in [egress policies](./egress-policies.md#actions), with the job's `Workload` as principal and its `avirt.internal/build` tag distinguishing it. avirtedgeproxyd evaluates avirtregistryd's and avirtbuilderd's off-host connections as it does every host daemon's.

```cedar
@id("team-a-builds-from-own-org")
permit (principal, action == Action::"build_workspace", resource in Namespace::"team-a")
when { ["github.com"].containsAll(context.source_hosts) && ["team-a"].containsAll(context.github_owners) };

@id("build-jobs-reach-npm")
permit (principal, action == Action::"connect", resource)
when { principal.hasTag("avirt.internal/build") && resource has host && resource.host == "registry.npmjs.org" && resource.port == 443 };
```

## Trust boundaries

| Listener | Where | Transport | Caller identity | Allowed operations |
| --- | --- | --- | --- | --- |
| avirtbuilderd (`services/avirtbuilderd.sock`) | Host, avirtbuilderd | Host service listener | avirtapid with `x-avirt-caller`, avirthostd (job exit reports) | `WorkspaceBuilder` calls, each authorized by Cedar |

| Compromised | Can reach | Cannot reach |
| --- | --- | --- |
| Build job (guest root in the build VM) | Its own inputs, its staging repository, its cache scope, and what its workload policy snapshot allows through avirtedgeproxyd. Writing anything into its staging output. | Any workspace repository, tag or annotation (only avirtbuilderd promotes), another build's inputs or staging, real credentials, avirtapid, avirthostd's control API, any signing key |
| avirtbuilderd | Starting build jobs, promoting what it chooses, and writing any annotation | Running commands on the host, any SVID other than its own, avirthostd's control API beyond creating build jobs |

# Open questions

1. **Webhook delivery.** avirtbuilderd derives events by polling. Whether GitHub webhooks can reach a host, through a relay or through a peer, is open.
2. **Secret content scanning.** The denylist matches paths. Whether avirtbuilderd also scans layer content for credentials before promotion, and whether a finding fails the build or only flags it, is open.
3. **SSH-only remotes.** A remote with no HTTPS form cannot be fetched through avirtedgeproxyd with credential injection. Whether SSH remotes are supported, and how their keys are held, is open.
4. **Signing.** Whether avirtbuilderd's promoted images are signed, by what key, and whether `launch_workspace` can require a signature through `Referrers`, is open.
5. **Build job namespace.** A build job runs in the workspace's namespace. Whether builds get a separate namespace so that agent policy never matches them by namespace is open.
6. **Directory sources in prebuilds.** A prebuild refuses a `directory` source, even one whose `path` stays inside the built tree. Whether such a source reads from the fetched tree in a prebuild is open.
