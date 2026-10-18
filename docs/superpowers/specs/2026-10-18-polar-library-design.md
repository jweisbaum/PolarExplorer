# Polar library — design

Date: 2026-10-18. Asked for by the user the same day: "A polar library is
just the combination of all orc, orr, and imported polar files. Any tab will
have a 'Save to library' option for the blended polar. The library supports
polars in any format. The search feature in the top left of the orc/orr will
instead just search the library. When you save a tab to the library, you
pick which format and give the polar a name. There is only ever one global
library."

Settled with the user before this design was written:

- The Polar files section merges into the library section, and **Import…**
  adds a file to the library and to the boat at once.
- A polar saved to the library carries the boat's identity (model, sail
  number, country, builder, designer, year built), prefilled from the tab's
  ORC or ORR source and editable, so field search and race-project matching
  cover it.
- The library lives in the application's own config folder. Nothing to
  configure, one per computer.

Approved in conversation on 2026-10-18 with two defaults left as proposed:
removing a library polar is permanent (not undoable), and saving under a
name the library already holds asks whether to replace that entry.

## 1. What the library is

The polar library is the one global set of polars a person can add to any
boat tab: the embedded and scraped ORC catalogue (spec §5), the ORR
catalogue (§5.5), and **library files** — polar files the person imported or
blends they saved, kept as files in the formats the application reads and
writes (§6). The two catalogues are unchanged by this design. The library
files are the new third list, and the one search covers all three.

The library is not project data. A project holds copies (sources), as it
does for certificates today (invariant 1), and is never changed by anything
done to the library.

## 2. Storage

`<config_dir>/polar-library/` beside `settings.json` and
`orr-catalogue.json`, created on first use:

```
polar-library/
  index.json
  files/<sanitised name>.<ext>
```

`index.json`:

```json
{
  "schema_version": 1,
  "next_id": 12,
  "entries": {
    "3": {
      "name": "Fastnet 2025 blend",
      "format": "expedition",
      "file": "files/Fastnet 2025 blend.txt",
      "sha256": "…",
      "added": 1760800000,
      "origin": { "saved": { "project": "Fastnet polar", "boat": "Astarte" } },
      "boat": { "model": "JPK 1180", "sail_no": "FRA 43800", "country": "FRA",
                "builder": "JPK", "designer": null, "year": 2019 },
      "axes": { "twa": 37, "tws": 11, "tws_min": 4.0, "tws_max": 24.0,
                "full_circle": false },
      "thumb": [ { "tws": 6.0, "points": [[52.0, 5.9], …] }, … ]
    }
  }
}
```

- **Ids** are `u64`, allocated from `next_id`, never reused. Entries are a
  `BTreeMap<u64, Entry>`; every `f64` goes through the `canonical` helpers.
- **`origin`** is `{"imported": {"file_name": "…"}}` for a file the person
  imported, or `{"saved": {"project", "boat"}}` for a blend saved from a tab.
- **`boat`** holds the identity fields, each optional. An imported file has
  none unless the person adds them later (not in this design; the fields are
  there for it).
- **`axes`** and **`thumb`** are computed once, when the entry is made, from
  the parsed polar: the number of angles and wind speeds, the wind range,
  whether it is full-circle, and the light, medium and strong wind curves
  the catalogue hits already show (`OrcThumbCurve`: the wind speeds nearest
  6, 12 and 20 kn). Search and listing read the index only; **Add** reads
  the file.
- **The file name** is the entry's name with the characters a file system
  refuses replaced by `_`, trimmed, limited to 120 bytes, then the format's
  extension; a name that is already a file in `files/` gets ` (2)`, ` (3)`,
  … The index is authoritative: a file in `files/` that no entry names is
  ignored (never deleted), and an entry whose file is missing is listed as
  **unavailable** and cannot be added, as the track library lists a track
  whose file is gone.
- **Writes are atomic** (`pe_core::io::write_atomic`), the file first, then
  the index. A failure between the two leaves an orphan file, which the
  rule above ignores. Replacing an entry (§4) rewrites its file in place,
  then the index. Removing an entry deletes its file, then writes the index;
  only files under `polar-library/files/` are ever deleted.
- **Loading** reads the index once into `AppState` (an `Arc`, replaced on
  every write, like the ORR records). A damaged index is an error named in
  the section and the library is treated as empty until it is repaired; it
  is never overwritten by a write made while damaged (a write then refuses).

The format value is `PolarFileFormat`'s serde name, the one the source
summary already uses (`expedition`, `adrena`, `csv`, `qtvlm`, `opencpn`,
`luckgrib`, `whirlwind_matrix_polar`).

## 3. Import

The left navigation's first two sections become one, **Polar library**
(§3.2's layout updates): the search box and **Search by field** (§5.2),
**Import…**, the boat's polar sources (ORC, ORR and files, each with its
colour, badge, detail line and **Remove**, undoable), and a footer counting
library polars, ORC certificates and ORR certificates. The Tracks section
follows as before.

**Import…** opens the native multi-file picker as today (§6). For each file,
in the order picked:

1. Read and parse it (§6's rules, unchanged: size, format detection,
   refusals naming line and column).
2. Hash the bytes. If an entry with that SHA-256 exists, it is reused:
   nothing is stored twice. Otherwise a new entry is made with
   `origin.imported.file_name`, no boat identity, and the file's bytes
   copied unchanged into `files/`.
3. Add a `PolarFile` source to the boat, labelled with the file name, with
   `library_id` set to the entry.

Everything a batch imports into the project is one undo entry, as today.
The library additions are not undone — the library is not project state —
and a file that failed to parse is not added to the library. Failures are
listed in the section as today.

`SourceKind::PolarFile` gains `library_id: Option<u64>` (serde default,
skipped when `None`). `SCHEMA_VERSION` becomes 9; the migration changes
nothing (older files read with `None`). A source whose `library_id` names
an entry that no longer exists is simply a source: nothing dangles.

## 4. Save to library

The Blend row of the source list (§8) gets **Save to library…** beside
**Export…**. Its dialog is the export dialog (§12.4) with, above the format:

- **Name**, required, defaulted to the boat's label (a boat tab's name).
- **Boat** fields: model, sail number, country, builder, designer, year
  built — prefilled from the boat's first visible ORC or ORR source (else
  its first of either; else empty), all editable, all optional.

Then the export dialog's own controls: format (all seven), the grid (project
or custom axes), the WhirlwindMatrixPolar mode and wave-bin controls, and
the preview. **Save** needs no native dialog: it writes exactly the bytes
**Export…** would write for the same choices, recomputed from the sources
and overlays (invariant 2), into the library, then makes the entry with
`origin.saved = {project name, boat label}`, the boat fields, and the
`axes` and `thumb` of the written polar read back.

A name the library already holds (compared exactly, after trimming) asks
**Replace "…"?** Replacing rewrites that entry's file (its extension may
change with the format), fields and id-stable entry in place; sources in
projects are copies and are untouched. Cancel saves nothing.

The export refusals (§12.4: axis collision, out of range, over 60 kn, an
empty blend) apply unchanged, before anything is written. A saved file is
what its format can hold: two decimals, no matrix outside
WhirlwindMatrixPolar. Saving is the person's explicit export of a derived
polar, so the blend is still never stored as truth; the file is one more
immutable source.

## 5. Search

The one box and the field boxes of §5.2 search three lists: the ORC
catalogue, the ORR catalogue and the library files. Every rule of §5.2
holds for the library list:

- A library entry's fields are its **name**, and its boat **model**, **sail
  number** (with or without the country, compact forms), **country**,
  **builder**, **designer** and **year built** (the year-built bounds leave
  out an entry without a year while either is set), plus its **format
  name** ("Expedition", "Adrena", …) and origin project and boat names as
  words of the main box only. The certificate-year box matches no library
  entry.
- Tokens are ANDed, each starting a word, case- and accent-insensitive; the
  field boxes match their field only.
- Ranking within the library list: a field box equal to its whole field,
  then exact sail number of the main box, then name prefix, then model
  prefix, then other matches; within each, newest `added` first, then by
  name.
- The three ranked lists are **interleaved — library, ORC, ORR, library,
  ORC, ORR, …** — each in its own rank, fifty of each a page, the next page
  loading as the list is scrolled (the existing mechanism, a third list).
  Each hit is badged **Library**, **ORC** or **ORR**. Totals are added.
- **Browsing**: with the main box and every field box empty, the catalogues
  list nothing (as today) and the library lists all its entries, newest
  first, paged the same way. So an empty search is the library.
- A library hit shows the name, format, axes summary, the boat identity it
  has, the saved or imported date, the thumbnail, **Add** (disabled, badged
  **Added**, when the boat holds a source with that `library_id`; disabled
  with **Unavailable** when the file is missing) and **Remove**. Remove asks
  **Remove "…" from the library?** and is not undoable; sources already in
  projects are unaffected.
- **Add** reads the file through the import parser (§3 step 1) and adds a
  `PolarFile` source labelled with the entry's name, `library_id` set, one
  undo step. A file that no longer parses (edited by hand, say) is refused
  naming the entry and the line and column.
- **Budget**: §13's 30 ms per keystroke holds with the catalogues and one
  thousand library entries; the library index is searched from an in-memory
  per-field word index built when the index is loaded or written.

The ORC catalogue's `in_project`/**Added** marking and the duplicate
question of §5.3 are unchanged.

## 6. Race projects

Tracker projects (§3.0, `boats/tracker_project.rs`) build the profiles they
match against from the ORC and ORR catalogues and the track library's
vessels. Library entries whose `boat` holds a model join those profiles, with
the same fields (model, builder, sail number, year built) and the same
match modes and rejection rules. A match adds the entry as a `PolarFile`
source of that boat (through §5's Add) and counts in the report's `polars`.
An entry already added to that boat, or unavailable, is skipped. Nothing
else in the dialog changes.

## 7. MCP service

Four tools in a new group file `mcp/tools/polar_library.rs`, each calling
the `#[tauri::command]` itself, each taking the optional `boat`:

| Tool | Does |
|---|---|
| `polar_library_search` | `query`, the optional `filters` of `orc_search`, `limit`, `offset`: the interleaved hits of §5 with their badge and, for a library hit, its id, name, format, boat identity, availability and date. |
| `polar_library_add` | `id`: §5's Add, one undo step. Refused when unavailable. |
| `polar_library_save` | `name`, `format`, optional `axes`, optional `matrix`, optional `boat` identity fields, `overwrite` (a name already held is refused unless `overwrite: true`, as the dialog asks). Answers the entry. |
| `polar_library_remove` | `id`: §5's Remove. |

`orc_search`, `orr_search`, `orc_add`, `orr_add` and `polar_files_import`
stay as they are. `polar_files_import` adds to the library too (it is the
same command). The guide (`tools/guide.rs`) names the four tools and sends an
agent to `polar_library_search` first when it looks for a boat's polar,
since the person's own saved blends are there. The new IPC commands join
`invoke`'s `TABLE`; the save command joins `EXCLUDED` (it holds the overwrite
rule, like `export_polar`). `tests/mcp.rs` drives each tool over HTTP and
checks the document through the interface's own read.

## 8. Interface, help and text

- Section features move to a `polar-library:` prefix (`polar-library:search`,
  `…:fields`, the field boxes, `…:import`, `…:add`, `…:remove-source`,
  `…:remove-entry`, `…:more`); `nav:orc` and `nav:polar-files` become
  `nav:polar-library`. The Blend row's new button is `sources:save-library`;
  the dialog's controls are `save-library:*`, landing on that button.
- Help topics: a new **Polar library** topic (what it holds, where it is,
  import, save, search, remove); the **ORC polars** and **Polar files**
  topics stay for catalogue and format detail and point to it. Every
  `ui/src/help/locales/<lang>.ts` follows.
- Every new string is translated into all eight languages; the glossaries
  gain "polar library".
- `docs/USER-GUIDE.md`'s **Add sources** is rewritten around the library;
  **Blend, compare and export** gains Save to library.
- `spec.md`: §3.2's layout and left-navigation text; §5 is retitled
  **Polar library**, its §§5.1–5.5 kept as they are, and a new §5.6
  **Library files** carries §§2–6 of this design; §6 keeps the formats and
  says Import… adds to the library; §8 the new button; §12.4 Save to
  library; §3.7's tool table; §13's budget line. `plan.md` gets a milestone
  with status.

## 9. Tests

- `pe-app` library module: index save → load → save byte identity; ids
  never reused across removes; import deduplication by hash; a missing file
  is unavailable, not an error; an orphan file is ignored; a damaged index
  refuses writes; file-name sanitising and ` (2)` suffixing; the thumbnail
  equals the catalogue's for the same grid.
- Save: bytes written to the library equal `export_bytes_with_matrix` for
  the same choices, for every format; replace keeps the id; refusals happen
  before any write.
- Search: ranking within the library list; the three-way interleave; empty
  query lists the library newest first and the catalogues not at all; the
  field boxes; the format-name word; an unavailable entry still listed.
- Document: schema 8 → 9 migration round trip; `library_id` survives
  save/load; Add and Import are one undo step each (inverse tests).
- Race project: an entry with a model matches under each mode and is added
  once.
- MCP: each tool over HTTP; `overwrite` refusal; guide names every tool.
- UX (`tools/webdriver/ux/`): import a file, see it under Polar library and
  in the sources; save the blend to the library under a name; type the name
  and see the Library-badged hit; Add it to a second boat; Remove it.
- Performance (`npm run ui:perf` and a Rust bench test): a keystroke over
  the catalogues plus 1,000 library entries under 30 ms.

## 10. Out of scope

- Editing a library entry's name or boat identity after the fact.
- A library location chosen in Settings, or several libraries.
- Scanning Expedition's and Adrena's folders on first launch (raised and
  withdrawn 2026-10-18); the library makes it a small later addition.
- Undoing a removal from the library.
