//! The MCP service (spec.md 3.7, D29): a loopback endpoint through which a client
//! drives the application, on only while the setting says so.

pub mod capture;
pub mod clients;
pub mod desktop;
pub mod events;
pub mod invoke;
pub mod native;
pub mod server;
pub mod skill;
pub mod token;
pub mod tools;

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::settings::{McpSettings, McpStatus};

/// How long a client that keeps no session stays counted after its last
/// call: what `rmcp` gives an idle session, so the two kinds of client leave
/// the status bar alike.
const SESSIONLESS_IDLE: Duration = Duration::from_secs(300);

/// Who is connected, as the service counts it.
///
/// **This is the consent indicator's source** (spec.md 3.7): the status bar
/// shows the badge while `shown()` counts anyone, so every way a client can
/// drive the application has to be counted here.
#[derive(Debug, Default)]
struct Activity {
    /// Sessions that initialised and have not ended.
    sessions: u32,
    /// Until when a client that keeps no session counts as connected. The
    /// 2026-07-28 protocol has no `initialize` and no session: each request
    /// stands alone, so such a client is counted from its calls.
    sessionless_until: Option<Instant>,
    /// The last tool a client called, while one is counted.
    last_tool: Option<String>,
    /// Which run of the listener the counts belong to. Stopping the
    /// listener starts a new one, so a session of the old listener that
    /// ends later takes nothing from the new one's count.
    generation: u64,
}

impl Activity {
    fn shown(&self) -> events::McpActivity {
        let sessions = self.sessions + u32::from(self.sessionless_until.is_some());
        events::McpActivity {
            sessions,
            last_tool: self.last_tool.clone().filter(|_| sessions > 0),
        }
    }
}

/// The service's live state, managed by Tauri beside `AppState`.
#[derive(Debug, Default)]
pub struct McpService {
    running: Mutex<Option<server::Running>>,
    app_lock: Mutex<Option<std::fs::File>>,
    /// Why the last bind failed, shown in Settings.
    pub(crate) bind_error: Mutex<Option<String>>,
    /// Connected clients and the last tool called (status bar, Settings).
    activity: Mutex<Activity>,
    /// Screenshots asked of the frontend and not yet answered.
    pub captures: capture::Captures,
}

impl McpService {
    /// Reconciles the listener with the settings: start, restart or stop.
    ///
    /// Always restarts when on, because the token or the port may have
    /// changed and both are baked into the running listener. Generic over
    /// the Tauri runtime so the mock application used by the integration
    /// tests can drive the same path as the shipped `tauri::Wry` build.
    pub fn apply<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>, mcp: &McpSettings) {
        use tauri::Manager;
        let dir = app
            .state::<crate::commands::AppState>()
            .paths
            .config_dir
            .join("mcp");
        let ready = self
            .app_lock
            .lock()
            .map(|mut lock| {
                if lock.is_none() {
                    *lock = whirlwind_mcp::endpoint::try_lock(&dir).ok().flatten();
                }
                lock.is_some()
            })
            .unwrap_or(false);
        if !ready {
            self.set_bind_error(Some("Another app instance owns AI assistant access".into()));
            return;
        }
        whirlwind_mcp::endpoint::remove(&dir);

        // `take()` under the lock, then drop the guard before `stop()`:
        // `stop` can block briefly waiting for the accept loop, and nothing
        // else that touches `self.running` (`running_port`, `status`) must
        // wait on that.
        let previous = self.running.lock().ok().and_then(|mut slot| slot.take());
        if let Some(previous) = previous {
            previous.stop();
            self.forget_clients(app);
        }
        if !mcp.enabled {
            self.set_bind_error(None);
            return;
        }
        match server::start(app.clone(), mcp.port, mcp.token.clone()) {
            Ok(running) => {
                let endpoint = whirlwind_mcp::endpoint::Endpoint {
                    version: 1,
                    port: running.port,
                    token: mcp.token.clone(),
                    pid: std::process::id(),
                };
                if let Err(error) = whirlwind_mcp::endpoint::write(&dir, &endpoint) {
                    running.stop();
                    self.set_bind_error(Some(error.to_string()));
                    return;
                }

                self.set_bind_error(None);
                self.adopt(running);
            }
            // Shown in Settings; the setting stays on, so the next launch
            // tries again.
            Err(message) => self.set_bind_error(Some(message)),
        }
    }

    /// Holds a listener started elsewhere (the tests start one on port 0).
    pub fn adopt(&self, running: server::Running) {
        // `replace()` under the lock, then drop the guard before `stop()`,
        // for the same reason as `apply`: a concurrent `running_port` or
        // `status` read must not wait on the old listener's shutdown.
        let previous = self
            .running
            .lock()
            .ok()
            .and_then(|mut slot| slot.replace(running));
        if let Some(previous) = previous {
            previous.stop();
        }
    }

    /// The bound port while the listener is up.
    pub fn running_port(&self) -> Option<u16> {
        self.running
            .lock()
            .ok()
            .and_then(|r| r.as_ref().map(|r| r.port))
    }

    fn set_bind_error(&self, error: Option<String>) {
        if let Ok(mut slot) = self.bind_error.lock() {
            *slot = error;
        }
    }

    /// Nobody is connected any more: the listener stopped (switched off, or
    /// restarted with another token or port), so no client can reach the
    /// application however long `rmcp` keeps its idle sessions alive.
    fn forget_clients<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>) {
        use tauri::Emitter;
        if let Ok(mut activity) = self.activity.lock() {
            *activity = Activity {
                generation: activity.generation + 1,
                ..Activity::default()
            };
            let _ = app.emit(events::ACTIVITY, activity.shown());
        }
    }

    /// Records a tool call and tells the status bar. `in_session` is whether
    /// the caller is a counted session; a call from outside one counts its
    /// client as connected for [`SESSIONLESS_IDLE`].
    pub fn note_tool<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        name: &str,
        in_session: bool,
    ) {
        if !in_session {
            self.note_sessionless(app, SESSIONLESS_IDLE);
        }
        use tauri::Emitter;
        if let Ok(mut activity) = self.activity.lock() {
            activity.last_tool = Some(name.to_owned());
            let _ = app.emit(events::ACTIVITY, activity.shown());
        }
    }

    /// Counts a client that keeps no session as connected until `idle` has
    /// passed without another call from one.
    pub(crate) fn note_sessionless<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        idle: Duration,
    ) {
        let Ok(mut activity) = self.activity.lock() else {
            return;
        };
        let watched = activity.sessionless_until.is_some();
        activity.sessionless_until = Some(Instant::now() + idle);
        let generation = activity.generation;
        drop(activity);
        if watched {
            // The task below is already waiting, and reads the new deadline.
            return;
        }
        // Ends with the listener, so nothing of the service outlives its
        // switch (invariant 4).
        let stopped = self
            .running
            .lock()
            .ok()
            .and_then(|running| running.as_ref().map(|running| running.cancel.clone()))
            .unwrap_or_default();
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            use tauri::{Emitter, Manager};
            loop {
                let service = app.state::<McpService>();
                let deadline = match service.activity.lock() {
                    Ok(activity) if activity.generation == generation => activity.sessionless_until,
                    _ => None,
                };
                let Some(deadline) = deadline else { return };
                let now = Instant::now();
                if deadline <= now {
                    if let Ok(mut activity) = service.activity.lock()
                        && activity.generation == generation
                    {
                        activity.sessionless_until = None;
                        let _ = app.emit(events::ACTIVITY, activity.shown());
                    }
                    return;
                }
                tokio::select! {
                    () = stopped.cancelled() => return,
                    () = tokio::time::sleep(deadline - now) => {}
                }
            }
        });
    }

    /// A client session finished initialising. Answers the listener run it
    /// was counted in, for [`Self::session_closed`].
    pub fn session_opened<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>) -> u64 {
        use tauri::Emitter;
        let Ok(mut activity) = self.activity.lock() else {
            return 0;
        };
        activity.sessions = activity.sessions.saturating_add(1);
        let _ = app.emit(events::ACTIVITY, activity.shown());
        activity.generation
    }

    /// A session counted in listener run `generation` ended.
    pub fn session_closed<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>, generation: u64) {
        use tauri::Emitter;
        if let Ok(mut activity) = self.activity.lock()
            && activity.generation == generation
        {
            activity.sessions = activity.sessions.saturating_sub(1);
            let _ = app.emit(events::ACTIVITY, activity.shown());
        }
    }

    /// What Settings shows.
    pub fn status(&self, mcp: &McpSettings) -> McpStatus {
        let activity = self.activity.lock().ok().map(|activity| activity.shown());
        McpStatus {
            enabled: mcp.enabled,
            port: mcp.port,
            token: mcp.token.clone(),
            bound_port: self.running_port(),
            bind_error: self.bind_error.lock().ok().and_then(|e| e.clone()),
            sessions: activity.as_ref().map_or(0, |a| a.sessions),
            last_tool: activity.and_then(|a| a.last_tool),
            clients: clients::McpClient::available(),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used, reason = "test code")]
    use super::*;
    use tauri::Manager;
    use tauri::test::{mock_builder, mock_context, noop_assets};

    /// A client that keeps no session is counted from its calls, and stops
    /// being counted once it has made none for the idle time.
    #[tokio::test]
    async fn a_sessionless_client_stops_counting_once_idle() {
        let app = mock_builder()
            .manage(McpService::default())
            .build(mock_context(noop_assets()))
            .expect("mock app");
        let service = app.state::<McpService>();
        let sessions = || service.activity.lock().unwrap().shown().sessions;
        assert_eq!(sessions(), 0);
        // Generous times: a loaded runner oversleeps, and each check below
        // has half a second or more to spare.
        let idle = Duration::from_millis(1500);
        service.note_sessionless(app.handle(), idle);
        assert_eq!(sessions(), 1);
        // Another call before the time is up pushes it back, and is still
        // one client.
        tokio::time::sleep(Duration::from_millis(1000)).await;
        service.note_sessionless(app.handle(), idle);
        tokio::time::sleep(Duration::from_millis(1000)).await;
        assert_eq!(sessions(), 1, "the second call pushed the deadline back");
        tokio::time::sleep(Duration::from_millis(1500)).await;
        assert_eq!(sessions(), 0);
    }
}
