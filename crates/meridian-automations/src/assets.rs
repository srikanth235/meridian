//! Embedded SDK + runner assets, materialized into the user's automations dir.
//!
//! Bundling these in the binary means automations work out-of-the-box without
//! a separate `npm install` — the runtime layout under
//! `<workflow_dir>/automations/` looks like:
//!
//!   automations/
//!     package.json                                 — `{ "type": "module" }`
//!     .runtime/
//!       runner.mjs                                 — entry that imports the user file
//!     node_modules/@symphony/automation/
//!       package.json
//!       index.mjs                                  — JS SDK shim
//!       index.d.ts                                 — types for the harness

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const RUNNER_MJS: &str = include_str!("../runtime-assets/runner.mjs");
const SDK_INDEX_MJS: &str =
    include_str!("../runtime-assets/node_modules/@symphony/automation/index.mjs");
const SDK_INDEX_DTS: &str =
    include_str!("../runtime-assets/node_modules/@symphony/automation/index.d.ts");
const SDK_PACKAGE_JSON: &str =
    include_str!("../runtime-assets/node_modules/@symphony/automation/package.json");

const ROOT_PACKAGE_JSON: &str = r#"{
  "name": "symphony-automations-workspace",
  "private": true,
  "type": "module"
}
"#;

pub struct LayoutPaths {
    pub root: PathBuf,
    pub runner: PathBuf,
    pub sdk_dts: PathBuf,
}

pub fn install_runtime(automations_dir: &Path) -> io::Result<LayoutPaths> {
    fs::create_dir_all(automations_dir)?;
    let runtime_dir = automations_dir.join(".runtime");
    let sdk_dir = automations_dir
        .join("node_modules")
        .join("@symphony")
        .join("automation");
    fs::create_dir_all(&runtime_dir)?;
    fs::create_dir_all(&sdk_dir)?;

    write_if_changed(&automations_dir.join("package.json"), ROOT_PACKAGE_JSON)?;
    let runner = runtime_dir.join("runner.mjs");
    write_if_changed(&runner, RUNNER_MJS)?;
    write_if_changed(&sdk_dir.join("package.json"), SDK_PACKAGE_JSON)?;
    write_if_changed(&sdk_dir.join("index.mjs"), SDK_INDEX_MJS)?;
    let sdk_dts = sdk_dir.join("index.d.ts");
    write_if_changed(&sdk_dts, SDK_INDEX_DTS)?;

    Ok(LayoutPaths {
        root: automations_dir.to_path_buf(),
        runner,
        sdk_dts,
    })
}

pub fn sdk_index_dts() -> &'static str {
    SDK_INDEX_DTS
}

fn write_if_changed(path: &Path, contents: &str) -> io::Result<()> {
    if let Ok(existing) = fs::read_to_string(path) {
        if existing == contents {
            return Ok(());
        }
    }
    fs::write(path, contents)
}
