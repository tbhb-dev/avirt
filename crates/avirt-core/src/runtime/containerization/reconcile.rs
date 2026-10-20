//! Startup reconciliation, which only reaps.
//!
//! An avirthostd restart ends every workload, but avirthostd can die between any two steps of a
//! launch. The reaper runs under the state root's lock before avirthostd listens. It lists the
//! helpers and the launch intents on disk. Each helper it finds gets a stop, and a workload's
//! directories go only after the free lock shows that its helper has exited.

#[cfg(test)]
mod real_tests;
#[cfg(test)]
mod tests;

use super::intent::Intent;
use super::lifecycle::{HelperState, HelperStatus, HelperSupervisor};
use crate::runtime::Swept;
use crate::state::{self, remove_tree};
use core::time::Duration;
use std::fs;
use std::io;

/// How long the reaper gives a helper to stop its VM before it forces the helper.
const STOP_GRACE: Duration = Duration::from_secs(10);

/// Stops every helper the supervisor knows and removes the state of each workload that has ended.
///
/// A workload whose helper still runs after the stop stays in `kept`, with its directories on
/// disk, and so does every workload when the listing fails. The reaper leaves the caches and the
/// images alone.
pub async fn reap(supervisor: &dyn HelperSupervisor, state: &state::Root) -> Swept {
    let mut swept = Swept::default();
    match supervisor.list().await {
        Err(err) => {
            swept.failures.push(format!("could not list the helpers: {err:#}"));
            swept.kept = with_intent(state);
        }
        Ok(statuses) => {
            for status in statuses {
                reap_one(supervisor, state, status, &mut swept).await;
            }
            remove_orphans(state, &mut swept);
        }
    }
    swept
}

/// The workload ids under `workloads/` that have a launch intent.
fn with_intent(state: &state::Root) -> Vec<String> {
    let mut ids: Vec<String> = fs::read_dir(state.workloads())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| Intent::read(&entry.path()).is_ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    ids.sort();
    ids
}

/// Stops one helper if it's alive, and removes the workload's state after the helper exits.
async fn reap_one(supervisor: &dyn HelperSupervisor, state: &state::Root, status: HelperStatus, swept: &mut Swept) {
    let id = status.workload;
    let recorded = status.recorded.map_or_else(String::new, |launch| format!(" ({launch})"));
    let gone = match status.state {
        HelperState::Ended => Ok(()),
        HelperState::Incompatible(_) | HelperState::Running | HelperState::Unknown(_) => {
            supervisor.stop(&id, STOP_GRACE).await
        }
    };
    let removal = match gone {
        Ok(()) => remove_workload(state, &id),
        Err(err) => {
            swept.failures.push(format!("workload {id}{recorded}: {err:#}"));
            swept.kept.push(id);
            return;
        }
    };
    match removal {
        Ok(()) => swept.removed.push(id),
        Err(err) => {
            swept.failures.push(format!("workload {id}{recorded}: {err}"));
            swept.kept.push(id);
        }
    }
}

/// Removes the VM state, then the directory, of a workload whose helper has exited.
///
/// # Errors
///
/// Returns the error from the first removal that fails.
fn remove_workload(state: &state::Root, id: &str) -> io::Result<()> {
    remove_tree(&state.containers().join(id))?;
    remove_tree(&state.workloads().join(id))
}

/// Removes VM state with no workload directory beside it. Its helper's lock file went with the
/// directory, so no helper can hold it.
fn remove_orphans(state: &state::Root, swept: &mut Swept) {
    for entry in fs::read_dir(state.containers()).into_iter().flatten().flatten() {
        let name = entry.file_name();
        if state.workloads().join(&name).exists() {
            continue;
        }
        match remove_tree(&entry.path()) {
            Ok(()) => swept.removed.push(name.to_string_lossy().into_owned()),
            Err(err) => swept.failures.push(format!("{}: {err}", entry.path().display())),
        }
    }
}
