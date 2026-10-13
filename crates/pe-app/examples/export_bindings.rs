//! Regenerates the frontend's TypeScript bindings from the Rust types.
//!
//! `cargo run -p pe-app --example export_bindings`, or `npm run bindings`.
//!
//! Every type that crosses IPC must be listed here. CI runs this and fails if
//! the working tree changes, so bindings cannot drift from the Rust types.

use std::path::PathBuf;

use pe_app::autosave::RecoveredProject;
use pe_app::blend::{
    BlendCell, BlendSettingsInput, BlendSummary, ExportAxes, ExportPreview, ExportProblemView,
    ExportResult,
};
use pe_app::commands::AppInfo;
use pe_app::compare::CompareOperand;
use pe_app::env::{EnvEstimate, EnvJobTrack, EnvJobsStatus};
use pe_app::error::AppErrorPayload;
use pe_app::grib::{GribExportStatus, GribPreview};
use pe_app::orc::{OrcCatalogueInfo, OrcFilters, OrcHit, OrcSearchResult, OrcThumbCurve};
use pe_app::polar_edit::{EditOp, EditSurface, PolarCell};
use pe_app::polar_files::{PolarImportFailure, PolarImportResult};
use pe_app::polar_plot::{PolarCurve, PolarCurvePoint, PolarPlotResult};
use pe_app::polar3d::PolarNodeRef;
use pe_app::projects::{
    BoatInput, OrcSourceSummary, PolarFileSummary, ProjectSummary, RecentProject, SourceSummary,
};
use pe_app::settings::{AutosaveMode, LegacyCacheNotice, Settings};
use pe_app::trackers::{TrackerBoatRow, TrackerEventView, TrackerListed, TrackerProgress};
use pe_app::tracks::{
    CsvMappingInput, CsvPreview, SampleDetails, TrackBoatPreview, TrackFileInspection,
    TrackFileRequest, TrackFilters, TrackImportFailure, TrackImportLine, TrackImportResult,
    TrackSummary,
};
use pe_app::wave_split::{WaveSense, WaveSplit};
use ts_rs::{Config, TS};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Resolved against the manifest rather than the cwd, so the command works
    // from anywhere in the workspace.
    let out_dir: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ui/src/generated");
    std::fs::create_dir_all(&out_dir)?;

    // Ids, revisions, epoch seconds and byte counts are all 64-bit but never
    // approach 2^53, and bigint is painful to work with in React —
    // comparisons, JSON, template strings all need care. Plain numbers keep
    // the frontend simple.
    let cfg = Config::new()
        .with_out_dir(&out_dir)
        .with_large_int("number");
    pe_app::library::LibrarySettings::export_all(&cfg)?;
    pe_app::library::ScrapeProgress::export_all(&cfg)?;
    pe_app::library::database::MetadataProgress::export_all(&cfg)?;
    pe_app::library::BoatTrackSearch::export_all(&cfg)?;
    AppInfo::export_all(&cfg)?;
    AppErrorPayload::export_all(&cfg)?;
    ProjectSummary::export_all(&cfg)?;
    SourceSummary::export_all(&cfg)?;
    PolarFileSummary::export_all(&cfg)?;
    PolarImportResult::export_all(&cfg)?;
    PolarImportFailure::export_all(&cfg)?;
    PolarCurvePoint::export_all(&cfg)?;
    PolarCurve::export_all(&cfg)?;
    PolarCell::export_all(&cfg)?;
    EditSurface::export_all(&cfg)?;
    EditOp::export_all(&cfg)?;
    PolarPlotResult::export_all(&cfg)?;
    pe_app::polar_pdf::PolarPdfOptions::export_all(&cfg)?;
    pe_app::polar_pdf::PolarPdfResult::export_all(&cfg)?;
    BlendSummary::export_all(&cfg)?;
    BlendCell::export_all(&cfg)?;
    WaveSplit::export_all(&cfg)?;
    WaveSense::export_all(&cfg)?;
    // The MCP service (spec.md 3.7): its settings and status, and what it
    // tells the frontend.
    pe_app::settings::McpStatus::export_all(&cfg)?;
    pe_app::mcp::clients::McpClient::export_all(&cfg)?;
    pe_app::mcp::clients::McpRegistered::export_all(&cfg)?;
    pe_app::mcp::events::DocumentChanged::export_all(&cfg)?;
    pe_app::mcp::events::McpActivity::export_all(&cfg)?;
    pe_app::mcp::events::ViewStage::export_all(&cfg)?;
    pe_app::mcp::events::ViewBoat::export_all(&cfg)?;
    pe_app::mcp::events::ViewSelection::export_all(&cfg)?;
    pe_app::mcp::capture::CaptureRequest::export_all(&cfg)?;
    pe_app::orr::OrrCatalogueInfo::export_all(&cfg)?;
    pe_app::orr::OrrSearchResult::export_all(&cfg)?;
    pe_app::orr::OrrProgress::export_all(&cfg)?;
    pe_app::orc::OrcProgress::export_all(&cfg)?;
    pe_app::catalogues::CatalogueSettings::export_all(&cfg)?;
    BlendSettingsInput::export_all(&cfg)?;
    ExportAxes::export_all(&cfg)?;
    ExportPreview::export_all(&cfg)?;
    ExportProblemView::export_all(&cfg)?;
    ExportResult::export_all(&cfg)?;
    PolarNodeRef::export_all(&cfg)?;
    CompareOperand::export_all(&cfg)?;
    TrackSummary::export_all(&cfg)?;
    TrackerBoatRow::export_all(&cfg)?;
    TrackerEventView::export_all(&cfg)?;
    TrackerListed::export_all(&cfg)?;
    TrackerProgress::export_all(&cfg)?;
    EnvEstimate::export_all(&cfg)?;
    EnvJobTrack::export_all(&cfg)?;
    EnvJobsStatus::export_all(&cfg)?;
    GribPreview::export_all(&cfg)?;
    GribExportStatus::export_all(&cfg)?;
    TrackFilters::export_all(&cfg)?;
    CsvMappingInput::export_all(&cfg)?;
    CsvPreview::export_all(&cfg)?;
    TrackBoatPreview::export_all(&cfg)?;
    TrackFileInspection::export_all(&cfg)?;
    TrackFileRequest::export_all(&cfg)?;
    TrackImportFailure::export_all(&cfg)?;
    TrackImportLine::export_all(&cfg)?;
    TrackImportResult::export_all(&cfg)?;
    SampleDetails::export_all(&cfg)?;
    OrcSourceSummary::export_all(&cfg)?;
    OrcCatalogueInfo::export_all(&cfg)?;
    OrcFilters::export_all(&cfg)?;
    OrcHit::export_all(&cfg)?;
    OrcThumbCurve::export_all(&cfg)?;
    OrcSearchResult::export_all(&cfg)?;
    RecentProject::export_all(&cfg)?;
    BoatInput::export_all(&cfg)?;
    RecoveredProject::export_all(&cfg)?;
    AutosaveMode::export_all(&cfg)?;
    Settings::export_all(&cfg)?;
    pe_app::weather_cache::WeatherCacheStatus::export_all(&cfg)?;
    pe_app::boats::BoatTab::export_all(&cfg)?;
    pe_app::boats::BoatTabs::export_all(&cfg)?;
    pe_app::boats::BoatExportResult::export_all(&cfg)?;
    pe_app::boats::tracker_project::BoatImportProgress::export_all(&cfg)?;
    pe_app::boats::tracker_project::BoatMatchMode::export_all(&cfg)?;
    pe_app::boats::tracker_project::TrackerProjectResult::export_all(&cfg)?;
    LegacyCacheNotice::export_all(&cfg)?;

    // Keep generated files deterministic and free of ts-rs's trailing spaces.
    for entry in std::fs::read_dir(&out_dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|extension| extension == "ts") {
            let source = std::fs::read_to_string(&path)?;
            let clean = source
                .lines()
                .map(str::trim_end)
                .collect::<Vec<_>>()
                .join("\n");
            std::fs::write(path, format!("{clean}\n"))?;
        }
    }
    println!("bindings written to {}", out_dir.display());
    Ok(())
}
