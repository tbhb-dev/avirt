---
type: Component Design
title: macOS Keychain secrets provider
description: The macOS Keychain provider built into avirtsecretsd, with the login keychain and its access APIs through the security-framework crate, item classes and attributes, code signing and entitlement requirements, prompting and user-presence behavior for a background daemon, its unavailability in the all-in-one Linux image, and where avirt's own durable keys are kept.
tags: [avirt, secrets, avirtsecretsd, keychain, macos, provider, signing]
implementation: planned
generated: { by: codex/gpt-6, at: 2026-10-18T03:05:39Z }
---

# Status: planned

avirtsecretsd on macOS includes the `keychain` provider (see [secrets](./secrets.md#providers)). It reads and writes generic password items in the user's login keychain, and provides the bootstrap store for the credentials avirtsecretsd's other providers need, such as a 1Password service account token (see [1Password](./secrets-1password.md#service-account)).

## Keychain and APIs

avirtsecretsd uses the file-based login keychain, the user's default keychain, through the SecItem API of Security.framework, wrapped by the `security-framework` crate. It never sets `kSecUseDataProtectionKeychain`.

| Concern | Choice |
| --- | --- |
| Keychain | The login keychain (`SecKeychain::default()`), the only keychain that avirtsecretsd searches |
| Read | `ItemSearchOptions` with class `generic_password`, the service and the account, `load_data` and `load_attributes`, limit 1 |
| Write | `SecItemUpdate` of the item's data for an existing item, and `add_item` with `ItemAddOptions` for a new one |
| User interaction | Disabled for every runtime call, with `SecKeychain::disable_user_interaction()` held for the call (see [prompting](#prompting-and-user-presence)) |
| Data protection keychain | Not used (see [code signing and entitlements](#code-signing-and-entitlements)) |
| System keychain | Excluded. avirtsecretsd never runs as root. |

Constraint: the file-based keychain's access control is a per-item ACL that lists trusted applications by their code signing <!-- Apple code-signing term. --><!-- vale write-good.TooWordy = NO -->designated requirement<!-- vale write-good.TooWordy = YES -->. The data protection keychain's is an access group taken from the caller's entitlements. These models aren't interchangeable. An item in either model is invisible to the other.

## Items and references

A `keychain` reference is `keychain://<service>/<account>`, naming one generic password item (`kSecClassGenericPassword`) by its `kSecAttrService` and `kSecAttrAccount`. Both parts are percent-encoded when they contain `/`.

| Attribute | Value for items avirtsecretsd creates |
| --- | --- |
| `kSecAttrService` | `dev.avirt.secrets` |
| `kSecAttrAccount` | `<scope>/<name>` of the avirt secret, or the provider instance name for a bootstrap credential |
| `kSecAttrLabel` | `avirt: <scope>/<name>`, as Keychain Access shows it |
| `kSecAttrDescription` | `avirt secret` |
| `kSecValueData` | The value, as UTF-8 for a string and as raw bytes otherwise |

References to items that avirtsecretsd didn't create, such as those belonging to another app, use the item's own service and account. The provider excludes internet password, certificate, key, and identity items.

For an item's version, the provider uses `kSecAttrModificationDate`. Without change notifications from the Keychain, avirtsecretsd can't offer `watch` capability. avirtsecretsd polls by `cache_ttl`, with a default of 5 minutes for Keychain secrets.

## Code signing and entitlements

The file-based keychain works without entitlements. avirtsecretsd has no keychain entitlements. Only avirtvmd holds `com.apple.security.virtualization` (see [build and platforms](./build-and-platforms.md#signing)).

Constraint: a login keychain ACL trusts avirtsecretsd by its <!-- Apple code-signing term. --><!-- vale write-good.TooWordy = NO -->designated requirement<!-- vale write-good.TooWordy = YES -->, which must stay identical across builds and upgrades.

| Build | Signature | <!-- Apple code-signing term. --><!-- vale write-good.TooWordy = NO -->Designated requirement<!-- vale write-good.TooWordy = YES --> | Effect |
| --- | --- | --- | --- |
| Distribution | Developer ID, hardened runtime, notarized, with the signing identifier `dev.avirt.secretsd` | The identifier and the Developer ID team | Stable across releases, so an item trusted once stays trusted after every upgrade |
| Development | Ad-hoc | The binary's code directory hash | Changes with every build. The Keychain treats each rebuilt avirtsecretsd as a new app. Existing items refuse it until their ACLs trust it again |

A development avirtsecretsd uses the `env` or `file` provider for its bootstrap credentials by default. It uses the `keychain` provider only with a signature from a stable development identity.

Constraint: the data protection keychain is available only to code whose entitlements specify a keychain access group, and on macOS that entitlement requires a provisioning profile embedded in an app bundle. A bare ad-hoc-signed command-line binary can't include that entitlement.

## Prompting and user presence

avirtsecretsd is a background daemon, and no request path waits on a person at the Mac.

- **No prompts at run time.** Every runtime read and write runs with user interaction off. Where the Keychain would show a dialog, the call fails with `errSecInteractionNotAllowed`. The provider maps this error to `needs-user` for an ACL that doesn't trust avirtsecretsd, or to `locked` for a locked keychain.
- **Items avirtsecretsd creates.** `avirtadmin secret import` sends the value to avirtsecretsd, which creates the item itself. The Keychain adds the creating app to the item's ACL, so avirtsecretsd reads it later without a prompt.
- **Items another app created.** `avirtadmin secret authorize <scope>/<name>` makes avirtsecretsd read the item once with user interaction allowed. The Keychain then shows its access dialog in the user's session, and choosing Always Allow adds avirtsecretsd's <!-- Apple code-signing term. --><!-- vale write-good.TooWordy = NO -->designated requirement<!-- vale write-good.TooWordy = YES --> to the item's ACL. avirtadmin waits up to 60 s for the result.
- **Locked keychain.** The login keychain remains <!-- Keychain lock state. --><!-- vale ai-tells-experimental.VocabularySwap = NO -->unlocked<!-- vale ai-tells-experimental.VocabularySwap = YES --> during the user's login session, unless the user's keychain settings lock it after inactivity or at sleep. A locked keychain gives each Keychain secret the state `locked`, and avirtsecretsd serves these secrets only from cache. avirtsecretsd leaves the lock in place and never holds the login password.
- **User presence.** Touch ID and other user-presence access control (`SecAccessControl` with `.userPresence`) exist only in the data protection keychain, so the `keychain` provider has no user-presence items. Biometric approval is available through the 1Password desktop app instead (see [1Password](./secrets-1password.md#desktop-app)).

Constraint: the Keychain shows its dialog boxes only in a graphical login session. avirtsecretsd runs as a launchd agent in that session (see [secrets](./secrets.md#placement)). Started from an SSH session alone, or as a launchd daemon, it lacks dialog boxes and may lack an <!-- Keychain lock state. --><!-- vale ai-tells-experimental.VocabularySwap = NO -->unlocked<!-- vale ai-tells-experimental.VocabularySwap = YES --> login keychain, and `avirtadmin secret authorize` fails with `needs-user`.

## All-in-one Linux image

The `keychain` provider is unavailable in the all-in-one server image and on every non-macOS host. A configuration that specifies a `keychain://` reference there fails to load with an error that identifies the provider. The image's equivalents are:

| Use on the Mac host | All-in-one image |
| --- | --- |
| Bootstrap credentials in the login keychain | Files that the container platform mounts as secrets, such as `/run/secrets/<name>`, read with `file:` |
| Secrets imported into the login keychain | The same values in 1Password with a service account, or in mounted files |

## avirt's own durable secrets

The Keychain holds provider values and bootstrap credentials only. avirt's own durable keys stay where their owners keep them:

- The SPIFFE root key stays in avirthostd's state directory (`ca/`, 0600, see [identity](./identity.md#issuer)). avirthostd starts before avirtsecretsd and issues avirtsecretsd's SVID, so avirthostd never reads its root key through avirtsecretsd, and avirthostd itself doesn't read the Keychain.
- The issuing CA and every SVID key stay in memory, as identity describes.
- avirtsecretsd's own refresh state file (`state.journal`) excludes secret values (see [secrets](./secrets.md#persistent-state)).

# Open questions

1. **Hardware-bound root key.** Whether the SPIFFE root key on macOS becomes a Secure Enclave P-256 key, kept in `ca/` as the Secure Enclave's wrapped key blob so a copy of the state directory is useless on another Mac, and how its yearly rollover works then, is open.
2. **Development signing identity.** The choice of a stable signing identity for development builds of avirtsecretsd is open. The identity must let a developer's Keychain ACLs survive rebuilds.
3. **ACL repair after a signing change.** When the Developer ID team or the signing identifier changes, every item's ACL stops trusting avirtsecretsd. Whether avirtadmin re-authorizes every item in one step is open.
