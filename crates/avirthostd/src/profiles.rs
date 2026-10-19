//! Per-agent launch profiles specify the image and environment passed to the container. They
//! also specify any per-launch hook with cleanup (Codex's `CODEX_HOME` seed and write-back).

use std::collections::HashMap;
use std::env;
use std::path::Path;

use anyhow::Result;
use avirt_core::hostd::profile::{Cleanup, EnvVar, Prepared, Profile, Refused};

use crate::codex_auth::HostAuth;

/// The Claude profile.
struct Claude {
    /// The container image Claude runs in.
    image: String,
}

impl Profile for Claude {
    fn image(&self) -> &str {
        &self.image
    }

    fn prepare(&self, _scratch: &Path) -> Result<Prepared> {
        if env::var_os("CLAUDE_CODE_OAUTH_TOKEN").is_none_or(|token| token.is_empty()) {
            return Err(Refused(
                "CLAUDE_CODE_OAUTH_TOKEN is not set in avirthostd's environment; claude launches need it.".into(),
            )
            .into());
        }
        Ok(Prepared {
            cleanup: Cleanup::none(),
            env: vec![EnvVar::Inherit("CLAUDE_CODE_OAUTH_TOKEN")],
            shares: Vec::new(),
        })
    }
}

/// The Codex profile.
struct Codex {
    /// The host credentials that each launch copies into a private `CODEX_HOME`.
    auth: HostAuth,
    /// The container image Codex runs in.
    image: String,
}

impl Profile for Codex {
    fn image(&self) -> &str {
        &self.image
    }

    fn prepare(&self, scratch: &Path) -> Result<Prepared> {
        self.auth.prepare(scratch)
    }
}

/// The profiles by name.
pub type Profiles = HashMap<String, Box<dyn Profile>>;

/// Builds the profiles. Image names come from `CLAUDE_IMAGE` and `CODEX_IMAGE`.
///
/// # Errors
///
/// Returns an error if `HOME` is unset and `CODEX_HOST_HOME` is too.
pub fn all() -> Result<Profiles> {
    let image = |var: &str, default: &str| env::var(var).unwrap_or_else(|_| default.to_owned());
    let claude: Box<dyn Profile> = Box::new(Claude { image: image("CLAUDE_IMAGE", "avirt-workload-claude:dev") });
    let codex: Box<dyn Profile> =
        Box::new(Codex { image: image("CODEX_IMAGE", "avirt-workload-codex:dev"), auth: HostAuth::from_env()? });
    Ok(HashMap::from([("claude".to_owned(), claude), ("codex".to_owned(), codex)]))
}
