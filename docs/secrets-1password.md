---
type: Component Design
title: 1Password secrets provider
description: The 1Password provider for avirtsecretsd, a plugin built on the official 1Password Go SDK, with its reference syntax, service account and desktop app authentication, caching and rate limiting, the mapping from 1Password items to avirt secrets, egress, and its failure modes.
tags: [avirt, secrets, avirtsecretsd, 1password, provider]
implementation: planned
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-17T23:30:00Z }
---

# Status: planned

The 1Password provider gives avirtsecretsd read and write access to items in 1Password vaults. It is a provider plugin (see [secrets](./secrets.md#provider-plugin-interface)), so avirtsecretsd's own process never links 1Password code, and its behavior toward callers is the same as every other provider's.

## Plugin

The plugin is `avirtsecrets-1password`, a Go executable built on the official 1Password Go SDK (`github.com/1password/onepassword-sdk-go`) at a pinned version. avirtsecretsd starts one plugin process per configured 1Password instance and talks to it over the plugin protocol on a socket pair.

| Capability | Supported |
| --- | --- |
| `batch` | Yes, through the SDK's `Secrets().ResolveAll` |
| `write` | Yes, through the SDK's item API, conditional on the item version |
| `versions` | Yes, the 1Password item version |
| `watch` | No. avirtsecretsd polls by `cache_ttl`. |
| `interactive` | In desktop app mode only |

Constraints:

- The SDK is in version 0, and a new 0.x minor release can break its API. The plugin pins one minor release, and a move to another is a plugin release.
- Desktop app authentication in the Go SDK needs cgo. The macOS plugin is built with `CGO_ENABLED=1`, and the Linux plugin for the all-in-one image is built without it and supports service accounts only.
- The plugin is Developer ID signed and notarized with avirtsecretsd on macOS.

## Reference syntax

A 1Password secret's `ref` is a 1Password secret reference:

```text
op://<vault>/<item>/[<section>/]<field>[?attribute=<attribute>]
op://<vault>/<item>/[<section>/]<field>[?ssh-format=openssh]
```

| Part | Meaning |
| --- | --- |
| `<vault>` | The vault's name or its id |
| `<item>` | The item's title or its id |
| `<section>` | The section's name or id, when the field is in a section |
| `<field>` | The field's label or id |
| `attribute` | `otp` (or `totp`) for a one-time password field's current code |
| `ssh-format` | `openssh` for an SSH key field's private key in OpenSSH format |

Ids are stable across renames, and names are not. The plugin resolves a name to its id at its first fetch, logs the id in its `secret_fetch` events, and reports `not-found` when a later rename breaks a name. A reference whose name parts contain `/`, `?` or `#`, or that matches more than one item, is refused as `invalid`, and must use ids.

## Mapping to avirt secrets

Each avirt secret record names one 1Password reference. The record's `provider` is the 1Password instance, so one host can use several accounts:

```yaml
# secrets.d/op-work.yaml, the provider instance
provider: op-work
type: 1password
auth: service-account
account: example.1password.com
bootstrap: keychain://dev.avirt.secrets/op-work-service-account   # the service account token
plan: business                     # sets the local rate limits
cache_ttl: 1h

# secrets.d/github-deploy.yaml, a secret
name: github-deploy
scope: team-a
provider: op-work
ref: op://ixkq2b7hydgdqiu4lbwvzbnvpa/x3ed4tvpqofxnhdqcmi2nbbs3m/credential
kind: opaque
tags: {purpose: git}
```

| avirt secret `kind` | 1Password source |
| --- | --- |
| `opaque` | One field |
| `oauth` | One item, with its access token, refresh token and expiry in named fields (`access_token`, `refresh_token`, `expires_at`), which avirtsecretsd reads together and writes back together |
| `document` | A document item's file, or a file attached to a field |
| `tls` | A certificate field and a private key field of one item, named by two references in the record (`ref` and `key_ref`) |

`avirtadmin secret import` writes a value into a 1Password item through the plugin, as a `Write`, when the instance's credential may write to the vault.

## Authentication

The plugin authenticates to 1Password in one of two modes, set per instance.

### Service account

The default mode, and the only one in the all-in-one server image.

- The instance's `bootstrap` reference names the service account token. avirtsecretsd resolves it from a local provider (see [secrets](./secrets.md#bootstrapping)) and passes it to the plugin on an inherited file descriptor. The token is never in the plugin's environment, so the SDK's `OP_SERVICE_ACCOUNT_TOKEN` variable is not used.
- The service account is granted read access, and write access where an instance writes back, to the vaults its secrets name and to nothing else.
- A service account cannot read a Personal, Private or Employee vault. A secret in one of those vaults is reachable only in desktop app mode.

### Desktop app

Available on the Mac host only, for a person's own account.

- The instance names the account as the 1Password desktop app shows it (`account: example.1password.com`), and needs no stored credential.
- The 1Password desktop app must be running and signed in, with its developer setting to integrate with other apps turned on. Touch ID, when enabled in the app, is the approval method.
- The first fetch after the plugin starts, and the first after 10 minutes without a fetch, makes the desktop app show an approval prompt naming the requesting process. The plugin waits up to 60 s for approval and then fails the fetch with `needs-user`. Locking the app also ends the authorization.
- The approval covers the whole account for that process. The desktop app identifies the process by its executable path and cannot verify a third-party signature, so the person approving checks that the prompt names the installed `avirtsecrets-1password` path.
- avirtsecretsd keeps an approved authorization alive only while secrets with live leases need refreshing. A fetch on an idle instance therefore prompts again.

## Egress

The plugin's connections to 1Password go through avirtsecretsd's local egress socket and leave the host through avirtedgeproxyd, as `/service/avirtsecretsd` (see [secrets](./secrets.md#provider-plugin-interface)). The host policy snapshot permits them for the instance's 1Password sign-in domain only:

```cedar
@id("secretsd-1password")
permit (principal == HostService::"avirtsecretsd", action == Action::"connect", resource)
when {
  context.via == "service" && resource has host && resource.port == 443 &&
  (resource.host like "*.1password.com" || resource.host like "*.1passwordusercontent.com")
};
```

In desktop app mode the plugin talks to the desktop app over local IPC, and the app makes its own connections outside avirt.

## Caching and rate limiting

1Password limits service accounts per token per hour, and each 1Password account per day across all its service accounts:

| Plan | Reads per hour, per token | Writes per hour, per token | Requests per 24 hours, per account |
| --- | --- | --- | --- |
| Business | 10,000 | 1,000 | 50,000 |
| Teams | 1,000 | 100 | 5,000 |
| 1Password, Families | 1,000 | 100 | 1,000 |

The plugin and avirtsecretsd keep within them:

- **Client-side rate limit.** The plugin enforces a token bucket per instance, set by the instance's `plan` (default: the 1Password and Families row), at 80 % of each limit. A request beyond it fails with `rate-limited` locally and never reaches 1Password.
- **Batching.** avirtsecretsd sends every due fetch for one instance as one `Resolve`, which the plugin sends as one `ResolveAll`.
- **TTL.** The default `cache_ttl` for 1Password secrets is 1 hour and the cap is 24 hours. A secret with no live lease is fetched only on demand.
- **A 429 from 1Password.** The plugin reports `rate-limited` with the reset time 1Password returns, the instance's `Health` reports it until then, and avirtsecretsd serves stale values within `stale_ttl` meanwhile (see [secrets](./secrets.md#fetch-caching-and-refresh)).
- **Writes.** Write-back to 1Password counts against the write limit, and an `oauth` secret refreshed more often than the write limit allows is refused when its record loads.

The desktop app mode has no published request quota. The plugin applies the same client-side rate limit in that mode.

## Failure modes

| Failure | Plugin result | Effect |
| --- | --- | --- |
| The service account token is invalid, expired or deleted | `unauthenticated` for every fetch | The instance is `unauthenticated` until an operator replaces the bootstrap credential and restarts it. Cached values serve until they expire. |
| The service account lost access to a vault | `denied` for that vault's secrets | Those secrets fail, others continue |
| The item, section or field is gone or renamed | `not-found` | That secret fails, negatively cached for 30 s |
| A name matches more than one item | `invalid` | That secret fails until its reference uses ids |
| The hourly or daily limit is reached | `rate-limited`, with the reset time | Stale values within `stale_ttl`, then 503 for that instance's secrets |
| 1Password is unreachable, or avirtedgeproxyd refuses the connection | `unavailable` | Stale values within `stale_ttl`, then 503 |
| Desktop app not running, not signed in, locked, or the prompt not approved within 60 s | `needs-user` | The fetch fails at once. Cached values serve until they expire. |
| A conditional write finds a newer item version | `conflict` | An owned secret becomes `needs-login` (see [secrets](./secrets.md#write-back)) |
| The plugin crashes or hangs past a 30 s call deadline | No result | avirtsecretsd kills and restarts it with backoff, and the instance is `unavailable` meanwhile |
| The plugin's protocol version is unsupported | Refused at `Describe` | The instance is `degraded`, and its secrets fail |

# Open questions

1. **Rust in every avirt binary.** The plugin is Go, because 1Password publishes no official Rust SDK. Whether avirt accepts a Go plugin beside its Rust binaries, or drives the `op` CLI from a Rust plugin instead, is open.
2. **Request counting.** Whether one `ResolveAll` of several references counts as one request or as one per reference against the service account limits is open, and the client-side rate limit counts one per reference until it is settled.
3. **Proxy support.** Whether every HTTP request the Go SDK makes honors `HTTPS_PROXY` is open, and the plugin must be verified to make no direct connection.
4. **1Password Connect.** Whether the plugin also supports a self-hosted 1Password Connect server, which caches vaults locally and has no published request quota after its first sync, is open.
5. **1Password Environments.** Whether the plugin reads 1Password Environments as well as items is open.
