//! The sources group (spec.md 3.7): what a boat's polar is blended from,
//! and adding to it from the catalogues and from polar files.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use tauri::Manager;

use super::{BoatParams, PolarExplorer, ToolError, ToolResult, absolute, json, typed};
use crate::commands::AppState;
use crate::orc::OrcFilters;

/// A search answers this many certificates unless asked for another number.
const DEFAULT_LIMIT: u32 = 20;
/// The most one search answers: more is a page, asked for with `offset`.
const MAX_LIMIT: u32 = 100;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceSetParams {
    /// The boat's id, from boats_list. Without it, the first boat.
    #[serde(default)]
    pub boat: Option<u64>,
    /// The source's id, from sources_list.
    pub source: u64,
    /// A new label.
    #[serde(default)]
    pub label: Option<String>,
    /// A new colour, `#rrggbb`.
    #[serde(default)]
    pub colour: Option<String>,
    /// Shown or hidden. A hidden source is left out of the blend and of
    /// every plot.
    #[serde(default)]
    pub visible: Option<bool>,
    /// The blend weight, 0 to 1 (1 is the default). 0 takes the source out
    /// of the blend while leaving it drawn.
    #[serde(default)]
    pub weight: Option<f64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceRefParams {
    /// The boat's id, from boats_list. Without it, the first boat.
    #[serde(default)]
    pub boat: Option<u64>,
    /// The source's id, from sources_list.
    pub source: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceMoveParams {
    /// The boat's id, from boats_list. Without it, the first boat.
    #[serde(default)]
    pub boat: Option<u64>,
    /// The source's id, from sources_list.
    pub source: u64,
    /// Its new position in the list, 0 first. Display order only: the
    /// blend does not depend on it.
    pub to: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CatalogueSearchParams {
    /// The boat's id, from boats_list. Without it, the first boat.
    #[serde(default)]
    pub boat: Option<u64>,
    /// Words to find: boat name, sail number, model, builder, designer or
    /// year. Each word must start a word of the certificate.
    #[serde(default)]
    pub query: String,
    /// Optional field filters, an object: {"year_min": 2010, "year_max":
    /// 2020, "country": "GBR", "name": "", "sail_no": "", "model": "",
    /// "builder": "", "designer": "", "certificate_year": "2025"}. Every
    /// field is optional.
    #[serde(default)]
    pub filters: Option<Value>,
    /// How many to answer, at most 100 (20 when omitted).
    #[serde(default)]
    pub limit: Option<u32>,
    /// How many of the best matches to skip: the next page.
    #[serde(default)]
    pub offset: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OrcAddParams {
    /// The boat's id, from boats_list. Without it, the first boat.
    #[serde(default)]
    pub boat: Option<u64>,
    /// The certificate's `id` from orc_search.
    pub id: u32,
    /// Add it even though the boat already holds this certificate.
    #[serde(default)]
    pub allow_duplicate: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OrrAddParams {
    /// The boat's id, from boats_list. Without it, the first boat.
    #[serde(default)]
    pub boat: Option<u64>,
    /// The certificate's `id` from orr_search.
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OrrRefreshParams {
    /// The certificate year to download, 2018 or later.
    pub year: i32,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PolarFilesParams {
    /// The boat's id, from boats_list. Without it, the first boat.
    #[serde(default)]
    pub boat: Option<u64>,
    /// Polar files to import: Expedition .txt, Adrena .pol or CSV. Each
    /// path absolute, or beginning with `~/`.
    pub paths: Vec<String>,
}

/// The fields of [`OrcFilters`]. Every one is optional there, so a key it
/// does not have would be dropped without a word and the search answered
/// unfiltered; `filters` refuses it instead.
const FILTER_KEYS: [&str; 11] = [
    "year_min",
    "year_max",
    "country",
    "name",
    "sail_no",
    "model",
    "builder",
    "designer",
    "certificate_year",
    "size_min",
    "size_max",
];

fn filters(raw: Option<Value>) -> std::result::Result<OrcFilters, ToolError> {
    let Some(raw) = raw.filter(|value| !value.is_null()) else {
        return Ok(OrcFilters::default());
    };
    let given: serde_json::Map<String, Value> = typed("filters", raw)?;
    if let Some(unknown) = given
        .keys()
        .find(|key| !FILTER_KEYS.contains(&key.as_str()))
    {
        return Err(ToolError::Refused(format!(
            "filters: there is no {unknown:?} here; the keys are {}",
            FILTER_KEYS.join(", ")
        )));
    }
    typed("filters", Value::Object(given))
}

#[tool_router(router = tool_router_sources, vis = "pub(crate)")]
impl<R: tauri::Runtime> PolarExplorer<R> {
    #[tool(
        description = "A boat's sources and its blend. `sources`: each one's id, kind (orc, orr, polar_file, track), label, colour, visible, weight (0–1), count (cells of a polar, samples of a track) and `used` samples, `edits` it holds, and its certificate, file or track details. `blend`: the Blend entry's settings, output grid and coverage (direct, filled and empty cells)."
    )]
    async fn sources_list(&self, Parameters(p): Parameters<BoatParams>) -> ToolResult {
        let summary = self
            .run("sources_list", move |app| {
                crate::projects::project_summary(app.state(), p.boat)?
                    .ok_or(crate::error::AppError::NoProjectOpen)
            })
            .await?;
        json(&serde_json::json!({ "sources": summary.sources, "blend": summary.blend }))
    }

    #[tool(
        description = "Changes a source's label, colour, visibility or blend weight; give the ones to change. Weight runs 0 to 1: a source at 0.5 counts half as much in the blend as one at 1. Hidden sources are out of the blend and every plot. Each field given is one undo step."
    )]
    async fn source_set(&self, Parameters(p): Parameters<SourceSetParams>) -> ToolResult {
        if p.label.is_none() && p.colour.is_none() && p.visible.is_none() && p.weight.is_none() {
            return Err(ToolError::Refused(
                "there is nothing to change: give label, colour, visible or weight".to_owned(),
            ));
        }
        let summary = self
            .write("source_set", false, move |app| {
                let state = || app.state::<AppState>();
                // One command per field, as the interface's own controls
                // send them; the last one's summary is the answer.
                let mut last = None;
                if let Some(label) = p.label {
                    last = Some(crate::edit::set_source_label(
                        state(),
                        p.boat,
                        p.source,
                        label,
                    )?);
                }
                if let Some(colour) = p.colour {
                    last = Some(crate::edit::set_source_colour(
                        state(),
                        p.boat,
                        p.source,
                        colour,
                    )?);
                }
                if let Some(visible) = p.visible {
                    last = Some(crate::edit::set_source_visible(
                        state(),
                        p.boat,
                        p.source,
                        visible,
                    )?);
                }
                if let Some(weight) = p.weight {
                    last = Some(crate::edit::set_source_weight(
                        state(),
                        p.boat,
                        p.source,
                        weight,
                        None,
                    )?);
                }
                last.ok_or(crate::error::AppError::Internal(
                    "source_set had nothing to apply".to_owned(),
                ))
            })
            .await?;
        json(&summary)
    }

    #[tool(
        description = "Moves a source in the list. Display order only; the blend does not change."
    )]
    async fn source_move(&self, Parameters(p): Parameters<SourceMoveParams>) -> ToolResult {
        let summary = self
            .write("source_move", false, move |app| {
                crate::edit::move_source(app.state(), p.boat, p.source, p.to)
            })
            .await?;
        json(&summary)
    }

    #[tool(description = "Removes a source from the boat. undo puts it back, with its edits.")]
    async fn source_remove(&self, Parameters(p): Parameters<SourceRefParams>) -> ToolResult {
        let summary = self
            .write("source_remove", false, move |app| {
                crate::edit::remove_source(app.state(), p.boat, p.source)
            })
            .await?;
        json(&summary)
    }

    #[tool(
        description = "Searches the ORC rating-certificate catalogue: the one built into the application and what was scraped since (no network here): `total` matches and the best `hits`, each with the `id` orc_add takes, boat name, sail number, country, model, builder, designer, year and certificate year."
    )]
    async fn orc_search(&self, Parameters(p): Parameters<CatalogueSearchParams>) -> ToolResult {
        let filters = filters(p.filters)?;
        let limit = p.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let found = self
            .run("orc_search", move |app| {
                crate::orc::orc_search(app.state(), p.boat, p.query, filters, limit, p.offset)
            })
            .await?;
        json(&found)
    }

    #[tool(
        description = "Adds an ORC certificate's polar to the boat as a source, by the `id` orc_search gave. Refused if the boat already holds that certificate unless allow_duplicate is true."
    )]
    async fn orc_add(&self, Parameters(p): Parameters<OrcAddParams>) -> ToolResult {
        let summary = self
            .write("orc_add", false, move |app| {
                crate::orc::orc_add(app.state(), p.boat, p.id, p.allow_duplicate)
            })
            .await?;
        json(&summary)
    }

    #[tool(
        description = "Searches the ORR certificate catalogue kept on this computer (refreshed from Settings; no network here): `total` matches and the best `hits`, each with the `id` orr_add takes. Offshore and short-course polars of one boat are separate hits."
    )]
    async fn orr_search(&self, Parameters(p): Parameters<CatalogueSearchParams>) -> ToolResult {
        let filters = filters(p.filters)?;
        let limit = p.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let found = self
            .run("orr_search", move |app| {
                crate::orr::orr_search(
                    app.state(),
                    p.boat,
                    p.query,
                    filters,
                    limit,
                    p.offset.unwrap_or(0),
                )
            })
            .await?;
        json(&found)
    }

    #[tool(
        description = "Adds an ORR certificate's polar to the boat as a source, by the `id` orr_search gave. A certificate the boat already holds is not added twice."
    )]
    async fn orr_add(&self, Parameters(p): Parameters<OrrAddParams>) -> ToolResult {
        let summary = self
            .write("orr_add", false, move |app| {
                crate::orr::orr_add(app.state(), p.boat, p.id)
            })
            .await?;
        json(&summary)
    }

    #[tool(
        description = "Refreshes the ORC certificate catalogue from ORC's own service, data.orc.org: every country's valid certificates of the current year, about 60 MB in a minute or two, so tell the user before starting it. Certificates already held are not stored twice. Starts the download and answers at once with its progress (running, done and total countries, certificates read, added, updated, failures); read it later with invoke orc_scrape_status and stop it with invoke cancel_orc_scrape. Only needed when orc_search does not find a recent certificate: the application carries a catalogue of its own."
    )]
    async fn orc_refresh(&self) -> ToolResult {
        let progress = self
            .run("orc_refresh", move |app| {
                crate::orc::start_orc_scrape(app.clone(), app.state())
            })
            .await?;
        json(&progress)
    }

    #[tool(
        description = "Refreshes the ORR certificate catalogue kept on this computer for one certificate year, by downloading that year's certificates from www.regattaman.com: several minutes of requests to that site, so tell the user before starting it. Starts the download and answers at once with its progress (running, done, total, added, updated, failures); read it later with invoke orr_scrape_status and stop it with invoke cancel_orr_scrape. Only needed when orr_search does not find a recent certificate."
    )]
    async fn orr_refresh(&self, Parameters(p): Parameters<OrrRefreshParams>) -> ToolResult {
        let progress = self
            .run("orr_refresh", move |app| {
                crate::orr::start_orr_scrape(app.clone(), app.state(), None, p.year)
            })
            .await?;
        json(&progress)
    }

    #[tool(
        description = "Imports polar files (Expedition .txt, Adrena .pol, CSV) as sources of the boat, one undo step for all of them. Answers the `project` after the import, the file names `imported`, and `failures`: each file that was not read, with the line and column of the problem."
    )]
    async fn polar_files_import(&self, Parameters(p): Parameters<PolarFilesParams>) -> ToolResult {
        let paths = p
            .paths
            .iter()
            .map(|path| absolute(path))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let result = self
            .write("polar_files_import", false, move |app| {
                crate::polar_files::import_polar_files(app.state(), p.boat, paths)
            })
            .await?;
        json(&result)
    }
}
