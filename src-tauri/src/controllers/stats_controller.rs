// Backs the stats panel with a single command reading the running simulation's benchmark counters.
use crate::models::simulation::stats::StatsSnapshot;

use super::simulation_session_controller::SessionState;

fn snapshot_stats(state: &SessionState) -> Result<StatsSnapshot, String> {
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    let session = guard.as_ref().ok_or("no active simulation session")?;
    Ok(session.stats())
}

#[tauri::command]
pub fn get_stats(state: tauri::State<SessionState>) -> Result<StatsSnapshot, String> {
    snapshot_stats(&state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_stats_errors_with_no_active_session() {
        let state = SessionState::default();
        assert_eq!(snapshot_stats(&state).unwrap_err(), "no active simulation session");
    }
}
