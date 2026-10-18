//! The skill a client is given with the service (spec.md 3.7): a `SKILL.md`
//! saying when to reach for PolarExplorer and how.
//!
//! The server's instructions argue for the tools from inside the server's
//! own corner of the context, 2,048 characters of it in Claude Code and
//! possibly none elsewhere (`tools::guide`). A skill is the other place a
//! client looks when it decides how to do what it was asked: its
//! description is in front of the model from the first message, beside the
//! skills that write spreadsheets and PDFs — which is the company a request
//! for "a polar for this boat" otherwise keeps. VectorEffects added it when
//! sessions with the service connected were reported not to use it, even
//! asked to by name.
//!
//! It is not a second text to keep in step: the body **is**
//! [`guide::GUIDE`]. Only the description is its own, because it is read
//! where no tool is in sight and has to stand without them.
//!
//! Claude Code reads a personal skill from `~/.claude/skills/<name>/`, so
//! *Add to Claude Code* writes it there. Claude Desktop takes a skill as a
//! zip the person uploads, so *Add to Claude Desktop* writes the zip beside
//! the extension and the dialog says where. Nothing here opens a socket or
//! starts a process (invariant 4); it writes outside the application's own
//! folders, which is why it happens on that click and at no other time.

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::tools::guide;
use crate::error::{Context, Result};

/// The skill's name, which is also its folder's. Lowercase, as a skill's
/// name has to be.
pub const NAME: &str = "polarexplorer";

/// The zip written for Claude Desktop, in the application's own folder.
pub const ZIP_NAME: &str = "PolarExplorer-skill.zip";

/// `SKILL.md`: the front matter a client lists skills by, then the guide.
///
/// The description is one plain YAML scalar — no colon followed by a space,
/// no quotes to escape — and under the 1,024 characters a description may
/// run to; the test holds both.
pub fn skill_md() -> String {
    format!(
        "---\nname: {NAME}\ndescription: {}\n---\n\n# PolarExplorer\n\n{}\n\n## The tools\n\n{TOOLS}\n",
        description(),
        guide::GUIDE
    )
}

/// What a client shows the model of the skill before it is opened.
fn description() -> String {
    // The guide's own opening speaks of "these tools"; here none is in
    // sight, and the first sentence is what a request is matched against.
    "Use whenever the user names PolarExplorer, or wants a sailing polar (a boat's speed for each wind angle and wind speed) built, compared, corrected or exported for routing software such as Expedition or Adrena — from an ORC or ORR rating certificate, from polar files, or from how a boat actually sailed in a tracked race — or asks how a boat performed against its rating. PolarExplorer is an application open on this computer that does these through its MCP tools, so use them rather than writing code to compute or convert a polar. Says which tools to call, in which order.".to_owned()
}

/// Where the tools are, and what it means when they are not there.
const TOOLS: &str = "The tools belong to the MCP server named `polarexplorer` (in Claude Code their names begin `mcp__plugin_polarexplorer_polarexplorer__`; where tools are loaded on demand, search for that name). `polarexplorer_guide` returns the text above with today's date.\n\nThe native bridge opens the app when a tool needs it. If no such tool is listed, the plugin may need connecting or repair. Say so and ask the user to open PolarExplorer and turn on Settings > AI assistants, then try again. Do not build the polar some other way instead: what was asked for is the application's result, which the user can see and correct there, and a table computed by hand is not it.";

/// Where Claude Code keeps its configuration: `$CLAUDE_CONFIG_DIR`, or
/// `~/.claude`.
pub fn claude_home(home: &Path, config_dir_var: Option<OsString>) -> PathBuf {
    config_dir_var
        .filter(|value| !value.is_empty())
        .map_or_else(|| home.join(".claude"), PathBuf::from)
}

/// Writes `skills/polarexplorer/SKILL.md` under Claude Code's folder and
/// returns where. The folder is this application's by name, so what was
/// there is replaced: that is how a newer version's skill arrives.
pub fn install_for_claude_code(claude_home: &Path) -> Result<PathBuf> {
    let folder = claude_home.join("skills").join(NAME);
    std::fs::create_dir_all(&folder).doing("create", folder.display())?;
    let file = folder.join("SKILL.md");
    std::fs::write(&file, skill_md()).doing("write", file.display())?;
    Ok(file)
}

/// Writes the skill as the zip Claude Desktop's skill upload takes: one
/// folder named for the skill, holding `SKILL.md`.
pub fn write_zip(zip_file: &Path) -> Result<()> {
    let file = std::fs::File::create(zip_file).doing("write", zip_file.display())?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file(format!("{NAME}/SKILL.md"), options)
        .doing("write", zip_file.display())?;
    zip.write_all(skill_md().as_bytes())
        .doing("write", zip_file.display())?;
    zip.finish().doing("write", zip_file.display())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;

    /// The reference is the skill format, not this file: front matter
    /// between two `---` lines holding `name` (lowercase letters, digits and
    /// hyphens, 64 at most) and `description` (1,024 at most), each one line.
    #[test]
    fn the_front_matter_is_what_a_client_lists_skills_by() {
        let text = skill_md();
        let mut lines = text.lines();
        assert_eq!(lines.next(), Some("---"));
        let name = lines.next().expect("name");
        let description = lines.next().expect("description");
        assert_eq!(lines.next(), Some("---"));

        let name = name.strip_prefix("name: ").expect("name key");
        assert!(name.len() <= 64);
        assert!(
            name.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "{name}"
        );
        let description = description
            .strip_prefix("description: ")
            .expect("description key");
        assert!(description.chars().count() <= 1024);
        // A plain scalar: nothing YAML would read as a key, a comment, a
        // quote or a flow collection.
        assert!(!description.contains(": "), "{description}");
        assert!(!description.contains(" #"), "{description}");
        assert!(
            !description.starts_with(['"', '\'', '[', '{', '>', '|', '*', '&', '!', '%', '@', '`']),
            "{description}"
        );
        for said in ["PolarExplorer", "polar", "ORC", "Expedition", "race"] {
            assert!(description.contains(said), "no {said}: {description}");
        }
    }

    #[test]
    fn the_body_is_the_guide_and_says_what_missing_tools_mean() {
        let text = skill_md();
        assert!(text.contains(guide::GUIDE));
        assert!(text.contains("mcp__plugin_polarexplorer_polarexplorer__"));
        assert!(text.contains("Settings > AI assistants"));
    }

    #[test]
    fn claude_code_gets_the_file_where_it_reads_personal_skills() {
        let home = tempfile::tempdir().expect("tempdir");
        let claude = claude_home(home.path(), None);
        assert_eq!(claude, home.path().join(".claude"));
        let file = install_for_claude_code(&claude).expect("install");
        assert_eq!(
            file,
            home.path().join(".claude/skills/polarexplorer/SKILL.md")
        );
        // Again, over an older one: the button is repeatable.
        std::fs::write(&file, "old").expect("write");
        install_for_claude_code(&claude).expect("install again");
        assert_eq!(std::fs::read_to_string(&file).expect("read"), skill_md());
    }

    #[test]
    fn claude_home_prefers_the_variable_and_ignores_an_empty_one() {
        let home = Path::new("/home/someone");
        assert_eq!(
            claude_home(home, Some(OsString::new())),
            home.join(".claude")
        );
        assert_eq!(
            claude_home(home, Some(OsString::from("/elsewhere"))),
            PathBuf::from("/elsewhere")
        );
    }

    /// Read back by the zip reader, not by the writer's own bookkeeping.
    #[test]
    fn the_zip_holds_one_folder_named_for_the_skill() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join(ZIP_NAME);
        write_zip(&file).expect("zip");
        let mut archive =
            zip::ZipArchive::new(std::fs::File::open(&file).expect("open")).expect("a zip");
        assert_eq!(archive.len(), 1);
        let mut entry = archive.by_name("polarexplorer/SKILL.md").expect("entry");
        let mut text = String::new();
        entry.read_to_string(&mut text).expect("read");
        assert_eq!(text, skill_md());
    }
}
