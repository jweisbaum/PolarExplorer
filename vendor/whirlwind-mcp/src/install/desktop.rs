//! Windows Desktop registration. Its profile is separate from Claude Code's.
use super::{App, Homes, Host, Receipt, edit, receipt};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

const CONFIG: &str = "claude_desktop_config.json";

/// Prefer initialized MSIX data over the obsolete classic profile left by upgrades.
/// Never invent a profile or choose arbitrarily between multiple package profiles.
pub(super) fn profile(h: &Homes) -> Result<Option<PathBuf>> {
    let root = |variable: &str, fallback: &str| {
        if h.isolated_profile {
            h.home.join(fallback)
        } else {
            std::env::var_os(variable)
                .map(PathBuf::from)
                .unwrap_or_else(|| h.home.join(fallback))
        }
    };
    let local = root("LOCALAPPDATA", "AppData/Local");
    let roaming = root("APPDATA", "AppData/Roaming");
    let packages = local.join("Packages");
    let mut found = Vec::new();
    match fs::read_dir(&packages) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry?;
                if entry
                    .file_name()
                    .to_string_lossy()
                    .to_ascii_lowercase()
                    .starts_with("claude_")
                {
                    let dir = entry.path().join("LocalCache/Roaming/Claude");
                    if dir.is_dir() {
                        found.push(dir.join(CONFIG));
                    }
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e).context("Could not inspect Claude Desktop's Windows profiles"),
    }
    ensure!(
        found.len() <= 1,
        "Multiple Claude Desktop package profiles were found. Remove the unused Claude installation before connecting; no Claude settings were changed."
    );
    if let Some(path) = found.pop() {
        return Ok(Some(path));
    }
    let classic = roaming.join("Claude");
    Ok(classic.is_dir().then(|| classic.join(CONFIG)))
}

fn entry(app: &App, h: &Homes) -> Value {
    let mut args = vec![app.executable.display().to_string()];
    args.extend(app.args.clone());
    args.extend(["--client".into(), "claude_desktop".into()]);
    json!({"command":h.marketplace.join("plugins").join(&app.id).join("scripts/whirlwind-launch.exe"),"args":args})
}

fn same_launch(a: &Value, b: &Value) -> bool {
    a["command"].is_string() && a["command"] == b["command"] && a["args"] == b["args"]
}

fn document(text: &str) -> Result<Value> {
    // Empty files are created by some versions of Desktop's Edit Config command.
    let value: Value = if text.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(text.trim_start_matches('\u{feff}'))?
    };
    ensure!(
        value.is_object(),
        "Claude Desktop configuration must be a JSON object; it was left unchanged."
    );
    ensure!(
        value.get("mcpServers").is_none_or(Value::is_object),
        "Claude Desktop mcpServers must be a JSON object; it was left unchanged."
    );
    Ok(value)
}

/// Recognize Desktop's install paths without confusing it with the claude CLI.
fn desktop_process(path: &str) -> bool {
    let path = path.replace('\\', "/").to_ascii_lowercase();
    path.ends_with("/claude.exe")
        && path.split('/').any(|part| {
            part == "claude" || part == "anthropicclaude" || part.starts_with("claude_")
        })
}

pub(super) fn ensure_stopped(h: &Homes) -> Result<()> {
    if h.isolated_profile {
        return Ok(());
    }
    #[cfg(windows)]
    {
        // A fixed script with no interpolated paths or credentials. Desktop can
        // overwrite an external edit from its in-memory preferences while open.
        let system = std::env::var_os("SystemRoot").context("Windows SystemRoot is unavailable")?;
        let powershell =
            PathBuf::from(system).join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let script = "[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); $ErrorActionPreference = 'Stop'; $paths = @(Get-Process | Where-Object { $_.ProcessName -ieq 'Claude' } | ForEach-Object { if (!$_.Path) { throw 'Cannot inspect Claude process' }; $_.Path }); ConvertTo-Json -Compress -InputObject $paths";
        let output = crate::process::run(&powershell, &["-NoProfile".into(), "-NonInteractive".into(), "-Command".into(), script.into()], &h.codex, None, std::time::Duration::from_secs(15))
            .context("Could not check whether Claude Desktop is closed. Fully quit Claude Desktop and retry.")?;
        let paths: Vec<String> =
            serde_json::from_str(output.trim_start_matches('\u{feff}').trim())?;
        ensure!(
            !paths.iter().any(|path| desktop_process(path)),
            "Quit Claude Desktop completely, including its system tray icon, then retry. Claude can overwrite configuration changes while it is running."
        );
    }
    Ok(())
}

pub(super) fn connect(app: &App, h: &Homes, old: &Receipt) -> Result<(PathBuf, Value)> {
    let path = profile(h)?.context("Open Claude Desktop once to create its settings, fully quit it (including the system tray), then click Connect again.")?;
    ensure_stopped(h)?;
    let expected = entry(app, h);
    let mut installed = expected.clone();
    edit(&path, |text| {
        let mut doc = document(text)?;
        if let Some(existing) = doc.get("mcpServers").and_then(|s| s.get(&app.server)) {
            ensure!(
                same_launch(existing, &expected)
                    || old
                        .desktop_entry
                        .as_ref()
                        .is_some_and(|owned| same_launch(existing, owned)),
                "A Claude Desktop server named {} already uses a different launch command. It was left unchanged; rename or remove that entry in Claude before connecting.",
                app.server
            );
            installed = existing.clone();
            installed["command"] = expected["command"].clone();
            installed["args"] = expected["args"].clone();
        }
        if doc.get("mcpServers").is_none() {
            doc["mcpServers"] = json!({});
        }
        doc["mcpServers"][&app.server] = installed.clone();
        Ok(serde_json::to_string_pretty(&doc)? + "\n")
    })?;
    // The receipt needs only ownership of the launch command, never user env values.
    Ok((path, expected))
}

pub(super) fn registered(app: &App, h: &Homes) -> bool {
    let check = || -> Result<bool> {
        let saved = receipt(h, app, Host::ClaudeDesktop)?;
        let Some(path) = saved.desktop_config else {
            return Ok(false);
        };
        if profile(h)?.as_ref() != Some(&path) {
            return Ok(false);
        }
        let doc = document(&fs::read_to_string(&path)?)?;
        let expected = entry(app, h);
        Ok(same_launch(&doc["mcpServers"][&app.server], &expected)
            && expected["command"]
                .as_str()
                .is_some_and(|path| Path::new(path).is_file()))
    };
    check().unwrap_or(false)
}

pub(super) fn disconnect(app: &App, h: &Homes, old: &Receipt) -> Result<()> {
    let path = old.desktop_config.as_ref().context("This connection used an extension. Remove it in Claude Desktop's Extensions settings; Connect can then set up the tools automatically.")?;
    if !path.exists() {
        return Ok(());
    }
    ensure_stopped(h)?;
    edit(path, |text| {
        let mut doc = document(text)?;
        if let Some(existing) = doc.get("mcpServers").and_then(|s| s.get(&app.server)) {
            ensure!(
                old.desktop_entry
                    .as_ref()
                    .is_some_and(|owned| same_launch(existing, owned)),
                "The Claude Desktop entry was changed by another installer. It was left untouched."
            );
            if let Some(servers) = doc["mcpServers"].as_object_mut() {
                servers.remove(&app.server);
            }
            return Ok(serde_json::to_string_pretty(&doc)? + "\n");
        }
        Ok(text.to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::super::{atomic_write, receipt_path};
    use super::*;

    fn fixture() -> (tempfile::TempDir, App, Homes, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let (mut app, h) = super::super::tests::fixture(dir.path());
        app.executable = dir.path().join("Program Files/Boat App/boat.exe");
        atomic_write(&app.executable, b"fixture").unwrap();
        let config = h.home.join("AppData/Roaming/Claude").join(CONFIG);
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        atomic_write(
            &h.marketplace
                .join("plugins")
                .join(&app.id)
                .join("scripts/whirlwind-launch.exe"),
            b"fixture launcher",
        )
        .unwrap();
        (dir, app, h, config)
    }

    fn save_receipt(app: &App, h: &Homes, path: PathBuf, installed: Value) -> Receipt {
        let receipt = Receipt {
            version: app.version.clone(),
            executable: app.executable.clone(),
            desktop_config: Some(path),
            desktop_entry: Some(installed),
            ..Default::default()
        };
        atomic_write(
            &receipt_path(h, app, Host::ClaudeDesktop),
            &serde_json::to_vec(&receipt).unwrap(),
        )
        .unwrap();
        receipt
    }

    #[test]
    fn chooses_initialized_msix_over_classic_and_refuses_ambiguous_profiles() {
        let (_dir, _app, h, classic) = fixture();
        assert_eq!(profile(&h).unwrap(), Some(classic));
        let msix = h
            .home
            .join("AppData/Local/Packages/Claude_pzs8sxrjxfjjc/LocalCache/Roaming/Claude");
        fs::create_dir_all(&msix).unwrap();
        assert_eq!(profile(&h).unwrap(), Some(msix.join(CONFIG)));
        fs::create_dir_all(
            h.home
                .join("AppData/Local/Packages/Claude_other/LocalCache/Roaming/Claude"),
        )
        .unwrap();
        assert!(profile(&h).unwrap_err().to_string().contains("Multiple"));
    }

    #[test]
    fn missing_profile_does_not_create_a_fake_claude_installation() {
        let dir = tempfile::tempdir().unwrap();
        let (app, h) = super::super::tests::fixture(dir.path());
        assert!(profile(&h).unwrap().is_none());
        assert!(connect(&app, &h, &Receipt::default()).is_err());
        assert!(!h.home.join("AppData").exists());
    }

    #[test]
    fn connects_repairs_and_removes_only_owned_entry_with_exact_backups() {
        let (_dir, mut app, h, path) = fixture();
        let original = br#"{ "preferences": {"theme":"dark"}, "mcpServers":{"other":{"command":"keep","args":["me"]}} }"#;
        fs::write(&path, original).unwrap();
        let (target, installed) = connect(&app, &h, &Receipt::default()).unwrap();
        assert_eq!(target, path);
        assert_eq!(
            installed["args"],
            json!([app.executable, "--mcp", "--client", "claude_desktop"])
        );
        assert!(!installed.to_string().contains("token"));
        let before = fs::read(&path).unwrap();
        let old = save_receipt(&app, &h, target, installed);
        assert!(registered(&app, &h));
        connect(&app, &h, &old).unwrap();
        assert_eq!(
            fs::read(&path).unwrap(),
            before,
            "repeating Connect is idempotent"
        );
        let backups: Vec<_> = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(|e| {
                let p = e.ok()?.path();
                (p.extension().and_then(|s| s.to_str()) == Some("bak")).then_some(p)
            })
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(fs::read(&backups[0]).unwrap(), original);
        let mut doc = document(&fs::read_to_string(&path).unwrap()).unwrap();
        doc["mcpServers"][&app.server]["env"] = json!({"USER_SETTING":"keep"});
        fs::write(&path, doc.to_string()).unwrap();
        app.executable = h.home.join("Updated App/boat.exe");
        assert!(!registered(&app, &h));
        let (target, installed) = connect(&app, &h, &old).unwrap();
        assert!(installed.get("env").is_none());
        assert_eq!(
            document(&fs::read_to_string(&path).unwrap()).unwrap()["mcpServers"][&app.server]["env"]
                ["USER_SETTING"],
            "keep"
        );
        let updated = save_receipt(&app, &h, target, installed);
        assert!(registered(&app, &h));
        disconnect(&app, &h, &updated).unwrap();
        let doc = document(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(doc["preferences"]["theme"], "dark");
        assert_eq!(
            doc["mcpServers"],
            json!({"other":{"command":"keep","args":["me"]}})
        );
        assert!(!registered(&app, &h));
    }

    #[test]
    fn bad_json_and_foreign_same_name_entries_are_never_replaced() {
        let (_dir, app, h, path) = fixture();
        for original in [
            "{ broken",
            "[]",
            r#"{"mcpServers":null}"#,
            r#"{"mcpServers":{"test":{"command":"someone-else","args":[]}}}"#,
        ] {
            fs::write(&path, original).unwrap();
            assert!(connect(&app, &h, &Receipt::default()).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), original);
        }
    }

    #[test]
    fn removal_preserves_a_registration_replaced_by_someone_else() {
        let (_dir, app, h, path) = fixture();
        let (target, installed) = connect(&app, &h, &Receipt::default()).unwrap();
        let old = save_receipt(&app, &h, target, installed);
        let changed = r#"{"mcpServers":{"test":{"command":"foreign","args":[]}}}"#;
        fs::write(&path, changed).unwrap();
        assert!(disconnect(&app, &h, &old).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), changed);
    }

    #[test]
    fn missing_empty_and_bom_configurations_can_be_initialized() {
        let (_dir, app, h, path) = fixture();
        connect(&app, &h, &Receipt::default()).unwrap();
        for original in ["", "\u{feff}{\"preferences\":{\"keep\":true}}"] {
            fs::write(&path, original).unwrap();
            connect(&app, &h, &Receipt::default()).unwrap();
            assert!(
                document(&fs::read_to_string(&path).unwrap()).unwrap()["mcpServers"][&app.server]
                    .is_object()
            );
        }
    }

    #[test]
    fn desktop_process_detection_distinguishes_the_command_line_client() {
        for path in [
            r"C:\Program Files\WindowsApps\Claude_1.0_x64__id\app\Claude.exe",
            r"C:\Users\Jon\AppData\Local\AnthropicClaude\app-1.0\Claude.exe",
            r"C:\Program Files\Claude\Claude.exe",
        ] {
            assert!(desktop_process(path));
        }
        for path in [
            r"C:\Users\Jon\.local\bin\claude.exe",
            r"C:\tools\claude.exe",
            r"C:\Program Files\Claude\cowork-svc.exe",
        ] {
            assert!(!desktop_process(path));
        }
    }
}
