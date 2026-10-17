//! Per-agent launch profiles: the image, the env passed to the container, and any per-launch
//! hook with cleanup (Codex's `CODEX_HOME` seed and write-back).

use std::collections::HashMap;
use std::path::Path;

use agentvmm_core::controller::{Cleanup, Prepared, Profile, Refused};
use anyhow::Result;

use crate::codex_auth::HostAuth;

struct Claude {
    image: String,
}

impl Profile for Claude {
    fn image(&self) -> &str {
        &self.image
    }

    fn prepare(&self, _scratch: &Path) -> Result<Prepared> {
        if std::env::var_os("CLAUDE_CODE_OAUTH_TOKEN").is_none_or(|t| t.is_empty()) {
            return Err(Refused(
                "CLAUDE_CODE_OAUTH_TOKEN is not set in the controller's environment; claude launches need it.".into(),
            )
            .into());
        }
        // `-e NAME` with no value: the container CLI inherits the token from the controller's
        // environment, so it never appears on argv.
        Ok(Prepared {
            args: vec!["-e".into(), "CLAUDE_CODE_OAUTH_TOKEN".into()],
            inherit: vec!["CLAUDE_CODE_OAUTH_TOKEN"],
            cleanup: Cleanup::none(),
        })
    }
}

struct Codex {
    image: String,
    auth: HostAuth,
}

impl Profile for Codex {
    fn image(&self) -> &str {
        &self.image
    }

    fn prepare(&self, scratch: &Path) -> Result<Prepared> {
        self.auth.prepare(scratch)
    }
}

/// Image names come from `CLAUDE_IMAGE` and `CODEX_IMAGE`.
pub fn all() -> Result<HashMap<String, Box<dyn Profile>>> {
    let image = |var: &str, default: &str| std::env::var(var).unwrap_or_else(|_| default.to_owned());
    Ok(HashMap::from([
        (
            "claude".to_owned(),
            Box::new(Claude { image: image("CLAUDE_IMAGE", "agentvmm-workload-claude:dev") }) as Box<dyn Profile>,
        ),
        (
            "codex".to_owned(),
            Box::new(Codex { image: image("CODEX_IMAGE", "agentvmm-workload-codex:dev"), auth: HostAuth::from_env()? }),
        ),
    ]))
}
