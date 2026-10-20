//! PolarExplorer desktop application.
//!
//! Rust owns the entire domain; the webview is a view layer. Everything the
//! frontend can reach goes through a Tauri command in [`commands`],
//! [`projects`], [`edit`], [`env`], [`polar_files`], [`tracks`], [`trackers`], [`map_tracks`], [`polar_plot`], [`polar3d`], [`polar_edit`], [`blend`], [`compare`], [`orc`], [`settings`], [`basemap`],
//! [`quit`] or [`autosave`], and every failure it can see is an
//! [`error::AppError`].

pub mod autosave;
pub mod basemap;
pub mod blend;
pub mod boats;
pub mod catalogues;
pub mod commands;
pub mod compare;
pub mod derived;
pub mod edit;
pub mod env;
pub mod error;
pub mod grib;
pub mod library;
pub mod map_tracks;
pub mod matrix_export;
pub mod mcp;
pub mod menu;
pub mod orc;
pub mod orr;
pub mod paths;
pub mod polar3d;
pub mod polar_edit;
pub mod polar_files;
pub mod polar_pdf;
pub mod polar_plot;
pub mod priority;
pub mod projects;
pub mod quit;
pub mod reanalysis;
pub mod session;
pub mod settings;
pub mod trackers;
pub mod tracks;
pub mod wave_split;
pub mod weather_cache;

use tauri::Manager;

/// Starts the application. Returns only when the application exits.
pub fn run() -> anyhow::Result<()> {
    let paths = paths::AppPaths::resolve()?;
    // Fail loudly at startup rather than showing a blank map later.
    basemap::inspect(basemap::EMBEDDED)?;
    let builder = tauri::Builder::default().plugin(tauri_plugin_dialog::init());
    // The WebDriver endpoint (D25): an inbound socket on loopback that drives
    // the whole interface — invariant 4 in the other direction — so a shipped
    // build must not carry it, and the feature being off by default is what
    // makes sure of that (`tests/webdriver_optional.rs`).
    #[cfg(feature = "webdriver")]
    let builder = {
        eprintln!(
            "the WebDriver automation endpoint is compiled in; this build is for \
             testing and must not be shipped"
        );
        builder.plugin(tauri_plugin_webdriver_automation::init())
    };
    let app = builder
        .manage(commands::AppState::new(paths))
        .manage(mcp::McpService::default())
        .setup(|app| {
            let language = app
                .state::<commands::AppState>()
                .with_session(|session| Ok(session.settings.language.clone()))
                .unwrap_or_else(|_| settings::LANGUAGES[0].to_owned());
            app.set_menu(menu::build(app.handle(), &language)?)?;
            autosave::start(app.handle().clone());
            env::start(app.handle().clone());
            // The track library's startup scrape, only if the person chose it.
            library::on_startup(app.handle().clone());
            library::catalogue::warm(app.state::<commands::AppState>().inner().clone());
            // The catalogues' startup schedule, only if the person chose it
            // (spec.md 5.4).
            catalogues::on_startup(app.handle().clone());
            // One probe of Hindsight Sync's port on loopback (spec.md 3.4).
            reanalysis::on_startup(app.handle().clone());
            // The MCP service, only if the person left it on (spec.md 3.7).
            // With the setting off this starts nothing: no socket, no task.
            let mcp_settings = app
                .state::<commands::AppState>()
                .with_session(|session| Ok(session.settings.mcp.clone()));
            if let Ok(mcp_settings) = mcp_settings {
                app.state::<mcp::McpService>()
                    .apply(app.handle(), &mcp_settings);
            }
            Ok(())
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            menu::QUIT_ID => quit::request(app),
            menu::HELP_ID => {
                use tauri::Emitter;
                let _ = app.emit("help://open", ());
            }
            _ => {}
        })
        // The window's close button, and Close Window, ask first too.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if !quit::may_exit(&window.state::<commands::AppState>()) {
                    api.prevent_close();
                    quit::ask(window.app_handle());
                } else if catalogues::work_before_exit(window.app_handle()) {
                    api.prevent_close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            boats::boat_tabs,
            boats::add_boat,
            boats::rename_boat,
            boats::delete_boat,
            boats::restore_boat,
            boats::export_all_polars,
            boats::export_locked_boat_project,
            boats::import_boat_tab,
            boats::export_boat_tab,
            boats::tracker_project::open_tracker_project,
            boats::tracker_project::confirm_tracker_project,
            boats::tracker_project::discard_tracker_project,
            boats::tracker_project::boat_import_status,
            boats::tracker_project::cancel_boat_import,
            commands::app_info,
            library::set_library_settings,
            library::scrape::start_library_scrape,
            library::scrape::library_scrape_status,
            library::scrape::cancel_library_scrape,
            library::database::test_database_connection,
            library::database::start_metadata_download,
            library::database::metadata_download_status,
            library::database::cancel_metadata_download,
            library::catalogue::search_database_boats,
            library::catalogue::library_metadata_available,
            library::catalogue::import_database_track,
            basemap::basemap,
            projects::new_project,
            projects::open_project,
            projects::save_project,
            projects::save_project_as,
            projects::close_project,
            projects::project_summary,
            projects::recent_projects,
            projects::forget_recent_project,
            projects::clear_recent,
            edit::undo,
            edit::redo,
            edit::rename_project,
            edit::set_source_colour,
            edit::set_source_visible,
            edit::set_visible_sources,
            edit::set_source_weight,
            edit::set_source_label,
            edit::move_source,
            edit::remove_source,
            polar_files::import_polar_files,
            polar_plot::polar_plot,
            polar_plot::polar_plot_dots,
            polar_pdf::polar_pdf,
            tracks::inspect_track_files,
            tracks::inspect_csv_track,
            tracks::import_track_files,
            tracks::set_track_filters,
            tracks::set_track_derivation,
            tracks::set_track_wind,
            tracks::sample_details,
            trackers::tracker_event,
            trackers::cancel_tracker_event,
            trackers::import_tracker_boats,
            env::env_estimate,
            env::start_env_fetch,
            env::cancel_env_fetch,
            env::env_jobs,
            env::set_use_corrected,
            map_tracks::map_tracks,
            polar3d::polar_scene,
            polar3d::polar_scene_split,
            polar3d::set_excluded,
            polar_edit::polar_edit_surface,
            polar_edit::edit_polar,
            polar_edit::set_segment_statistic,
            blend::set_blend_visible,
            blend::set_blend_colour,
            blend::set_blend_settings,
            blend::set_global_filters,
            blend::set_wave_ranges,
            blend::wave_range_maxima,
            blend::set_priority_filters,
            blend::blend_cell,
            blend::blend_cell_split,
            blend::export_preview,
            blend::export_polar,
            compare::compare_polars,
            orc::orc_catalogue_info,
            orc::orc_search,
            orr::orr_catalogue_info,
            orr::orr_search,
            orr::orr_add,
            orr::orr_scrape_status,
            orr::start_orr_scrape,
            orr::cancel_orr_scrape,
            orc::orc_add,
            orc::orc_scrape_status,
            orc::start_orc_scrape,
            orc::cancel_orc_scrape,
            catalogues::set_catalogue_schedule,
            autosave::recovered_projects,
            autosave::open_recovered,
            autosave::discard_recovered,
            settings::app_settings,
            settings::set_language,
            settings::set_theme,
            settings::set_units,
            settings::set_autosave_mode,
            settings::set_weather_memory,
            settings::set_network,
            reanalysis::reanalysis_scan,
            reanalysis::reanalysis_probe,
            reanalysis::set_reanalysis_port,
            reanalysis::set_reanalysis_archive,
            weather_cache::set_weather_cache,
            weather_cache::weather_cache_status,
            weather_cache::clear_weather_cache,
            settings::set_projection,
            settings::set_plot_band,
            settings::mcp_status,
            settings::mcp_set,
            settings::mcp_rotate_token,
            mcp::clients::mcp_register_client,
            mcp::native::mcp_connections,
            mcp::native::mcp_connection_action,
            mcp::capture::deliver_capture,
            mcp::capture::refuse_capture,
            settings::legacy_cache_notice,
            settings::remove_old_chunk_cache,
            quit::quit_app,
        ])
        .build(tauri::generate_context!())?;
    app.run(|app, event| {
        // The platform's own quit (the Dock, logging out) arrives here with
        // no exit code; `app.exit` from `quit_app` arrives with one.
        if let tauri::RunEvent::ExitRequested { api, .. } = event {
            if !quit::may_exit(&app.state::<commands::AppState>()) {
                api.prevent_exit();
                quit::ask(app);
            } else if catalogues::work_before_exit(app) {
                api.prevent_exit();
            }
        }
    });
    Ok(())
}
