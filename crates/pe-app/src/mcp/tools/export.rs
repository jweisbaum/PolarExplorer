//! The export group (spec.md 3.7): writing the blend for routing software.
//! Every export recomputes the blend from the sources (invariant 2) and
//! writes the bytes the interface's own export writes (invariant 5).

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use tauri::Manager;

use super::{PolarExplorer, ToolError, ToolResult, absolute, json, may_write, typed};
use crate::blend::ExportAxes;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExportPreviewParams {
    /// The boat's id, from boats_list. Without it, the first boat.
    #[serde(default)]
    pub boat: Option<u64>,
    /// expedition, adrena, csv, qtvlm, opencpn, luckgrib, or whirlwind_matrix_polar.
    pub format: String,
    /// Matrix export options for whirlwind_matrix_polar: sog, sampled_pdf,
    /// gaussian_pdf, sog_bins (2–512), and optional waves. Defaults include all
    /// modes and 4×4×2 wave bins. waves:null omits waves. Custom height/period
    /// objects use {cutoffs:[...]}; angle uses {edges:[...]} spanning 360°.
    /// Defaults: height [1,3,5] m, period [5] s, angle [-45,45,135,225,315]°
    /// clockwise from the bow. Omitted fields retain their defaults.
    #[serde(default)]
    pub matrix: Option<Value>,
    /// Custom axes to read the blend onto instead of the output grid, an
    /// object: {"twa": [52, 60, 75, 90], "tws": [6, 8, 10]}.
    #[serde(default)]
    pub axes: Option<Value>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExportPolarParams {
    /// The boat's id, from boats_list. Without it, the first boat.
    #[serde(default)]
    pub boat: Option<u64>,
    /// Where to write the file: absolute, or beginning with `~/`.
    pub path: String,
    /// expedition, adrena, csv, qtvlm, opencpn, luckgrib, or whirlwind_matrix_polar.
    pub format: String,
    /// Matrix export options for whirlwind_matrix_polar: sog, sampled_pdf,
    /// gaussian_pdf, sog_bins (2–512), and optional waves. Defaults include all
    /// modes and 4×4×2 wave bins. waves:null omits waves. Custom height/period
    /// objects use {cutoffs:[...]}; angle uses {edges:[...]} spanning 360°.
    /// Defaults: height [1,3,5] m, period [5] s, angle [-45,45,135,225,315]°
    /// clockwise from the bow. Omitted fields retain their defaults.
    #[serde(default)]
    pub matrix: Option<Value>,
    /// Custom axes, as export_preview's.
    #[serde(default)]
    pub axes: Option<Value>,
    /// Replace a file that is already at `path`. Refused without it. Pass
    /// true only when the user means the file to be replaced.
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExportAllParams {
    /// The folder to write every boat's polar into: absolute, or beginning
    /// with `~/`.
    pub directory: String,
    /// expedition, adrena, csv, qtvlm, opencpn, luckgrib, or whirlwind_matrix_polar.
    pub format: String,
    /// Matrix export options for whirlwind_matrix_polar: sog, sampled_pdf,
    /// gaussian_pdf, sog_bins (2–512), and optional waves. Defaults include all
    /// modes and 4×4×2 wave bins. waves:null omits waves. Custom height/period
    /// objects use {cutoffs:[...]}; angle uses {edges:[...]} spanning 360°.
    /// Defaults: height [1,3,5] m, period [5] s, angle [-45,45,135,225,315]°
    /// clockwise from the bow. Omitted fields retain their defaults.
    #[serde(default)]
    pub matrix: Option<Value>,
}

fn axes(raw: Option<Value>) -> std::result::Result<Option<ExportAxes>, ToolError> {
    raw.filter(|value| !value.is_null())
        .map(|value| typed("axes", value))
        .transpose()
}

#[tool_router(router = tool_router_export, vis = "pub(crate)")]
impl<R: tauri::Runtime> PolarExplorer<R> {
    #[tool(
        description = "The blend as it would be exported, without writing anything: the grid (twa, tws, bsp), each cell's `origin` on the output grid, the file's `text`, and `problem` when the export would be refused (an empty blend, axis values that would collide when written with two decimals)."
    )]
    async fn export_preview(&self, Parameters(p): Parameters<ExportPreviewParams>) -> ToolResult {
        let axes = axes(p.axes)?;
        let matrix = p
            .matrix
            .filter(|v| !v.is_null())
            .map(|v| typed("matrix", v))
            .transpose()?;
        let preview = self
            .run("export_preview", move |app| {
                crate::blend::export_preview(app.state(), p.boat, p.format, axes, matrix)
            })
            .await?;
        json(&preview)
    }

    #[tool(
        description = "Writes the boat's blend as an Expedition (.txt), Adrena (.pol), qtVlm (.csv), OpenCPN (.pol), LuckGrib (.tsv), CSV (.csv) or WhirlwindMatrixPolar (.wmpolar) polar to `path`, recomputed from the sources at this moment. A file already there is replaced only with overwrite: true. Answers the file written and its size. The project does not need saving first."
    )]
    async fn export_polar(&self, Parameters(p): Parameters<ExportPolarParams>) -> ToolResult {
        let path = absolute(&p.path)?;
        may_write(std::path::Path::new(&path), p.overwrite)?;
        let axes = axes(p.axes)?;
        let matrix = p
            .matrix
            .filter(|v| !v.is_null())
            .map(|v| typed("matrix", v))
            .transpose()?;
        let written = self
            .run("export_polar", move |app| {
                tauri::async_runtime::block_on(crate::blend::export_polar(
                    app.state(),
                    p.boat,
                    path,
                    p.format,
                    axes,
                    matrix,
                ))
            })
            .await?;
        json(&written)
    }

    #[tool(
        description = "Writes every boat's blend into a folder, one file per boat, in one format. Answers the `paths` written and `failures`: each boat whose polar could not be exported, and why."
    )]
    async fn export_all(&self, Parameters(p): Parameters<ExportAllParams>) -> ToolResult {
        let directory = absolute(&p.directory)?;
        let matrix = p
            .matrix
            .filter(|v| !v.is_null())
            .map(|v| typed("matrix", v))
            .transpose()?;
        let written = self
            .run("export_all", move |app| {
                tauri::async_runtime::block_on(crate::boats::export_all_polars(
                    app.state(),
                    directory,
                    p.format,
                    matrix,
                ))
            })
            .await?;
        json(&written)
    }
}
