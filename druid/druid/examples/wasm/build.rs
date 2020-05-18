// Copyright 2020 The xi-editor Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::io::{ErrorKind, Result};
use std::path::{Path, PathBuf};
use std::{env, fs};

/// Examples known to not work with WASM are skipped. Ideally this list will eventually be empty.
const EXCEPTIONS: &[&str] = &[
    "svg",               // usvg doesn't currently build with WASM.
    "ext_event",         // WASM doesn't currently support spawning threads.
    "blocking_function", // WASM doesn't currently support spawning threads.
];

/// Create a platform specific link from `src` to the `dst` directory.
#[inline]
fn link_dir(src: &Path, dst: &Path) {
    #[cfg(unix)]
    link_dir_unix(src, dst);
    #[cfg(windows)]
    link_dir_windows(src, dst);
}

#[cfg(unix)]
fn link_dir_unix(src: &Path, dst: &Path) {
    let err = std::os::unix::fs::symlink(src, dst).err();
    match err {
        None => (),
        Some(err) if err.kind() == ErrorKind::AlreadyExists => (),
        Some(err) => panic!("Failed to create symlink: {}", err),
    }
}

#[cfg(windows)]
fn link_dir_windows(src: &Path, dst: &Path) {
    // First we have to delete any previous link,
    // especially because a junction is an absolute path reference
    // that becomes invalid if one of our ancestor directories gets renamed/moved.
    let err = fs::remove_dir(dst).err(); // Safe as it errors when directory isn't empty
    match err {
        None => (),
        Some(err) if err.kind() == ErrorKind::NotFound => (),
        Some(err) => panic!("Failed to remove directory: {}", err),
    }
    // Attempt to create a symlink, which will work with either
    // * Admininstrator privileges
    // * New enough Windows with developer mode enabled
    if std::os::windows::fs::symlink_dir(src, dst).is_ok() {
        return;
    }
    // Otherwise fall back to creating a junction instead,
    // by using Command Prompt's inbuilt 'mklink' command.
    std::process::Command::new("cmd")
        .arg("/C") // Run a command and exit
        .arg("mklink")
        .arg("/J") // Junction
        .arg(dst.as_os_str())
        .arg(src.as_os_str())
        .output()
        .expect("failed to execute process");
    // Make sure the directory exists now
    if !dst.exists() {
        panic!("Failed to create a link");
    }
}

fn main() -> Result<()> {
    let crate_dir = PathBuf::from(&env::var("CARGO_MANIFEST_DIR").unwrap());
    let src_dir = crate_dir.join("src");
    let examples_dir = src_dir.join("examples");

    let parent_dir = crate_dir.parent().unwrap();

    // Create a platform specific link to the examples directory.
    link_dir(parent_dir, &examples_dir);

    // Generate example module and the necessary html documents.

    // Declare the newly found example modules in examples.in
    let mut examples_in = r#"
// This file is automatically generated and must not be committed.

/// This is a module collecting all valid examples in the parent examples directory.
mod examples {
"#
    .to_string();

    let mut index_html = r#"
<!DOCTYPE html>
<html lang="en">
    <head>
        <meta charset="utf-8">
        <title>Druid WASM examples - index</title>
    </head>
    <body>
        <h1>Druid WASM examples</h1>
        <ul>
"#
    .to_string();

    for entry in examples_dir.read_dir()? {
        let path = entry?.path();
        if let Some(r) = path.extension() {
            if r != "rs" {
                continue;
            }
        } else {
            continue;
        }

        if let Some(example) = path.file_stem() {
            let example_str = example.to_string_lossy();

            // Skip examples that are known to not work with wasm.
            if EXCEPTIONS.contains(&example_str.as_ref()) {
                continue;
            }

            // Record the valid example module we found to add to the generated examples.in
            examples_in.push_str(&format!("    pub mod {};\n", example_str));

            // The "switch" example name would conflict with JavaScript's switch statement. So we
            // rename it here to switch_demo.
            let js_entry_fn_name = if &example_str == "switch" {
                "switch_demo".to_string()
            } else {
                example_str.to_string()
            };

            // Add an entry to the index.html file.
            let index_entry = format!(
                "<li><a href=\"./html/{name}.html\">{name}</a></li>",
                name = example_str
            );

            index_html.push_str(&index_entry);

            // Create an html document for each example.
            let html = format!(
                r#"
<!DOCTYPE html>
<html lang="en">
    <head>
        <meta charset="utf-8">
        <title>Druid WASM examples - {name}</title>
        <style>
            html, body, canvas {{
                margin: 0px;
                padding: 0px;
                width: 100%;
                height: 100%;
                overflow: hidden;
            }}
        </style>
    </head>
    <body>
        <noscript>This page contains webassembly and javascript content, please enable javascript in your browser.</noscript>
        <canvas id="canvas"></canvas>
        <script type="module">
            import init, {{ {name} }} from '../pkg/druid_wasm_examples.js';

            async function run() {{
                await init();
                {name}();
            }}

            run();
        </script>
    </body>
</html>"#,
                name = js_entry_fn_name
            );

            // Write out the html file into a designated html directory located in crate root.
            let html_dir = crate_dir.join("html");
            if !html_dir.exists() {
                fs::create_dir(&html_dir).unwrap_or_else(|_| {
                    panic!("Failed to create output html directory: {:?}", &html_dir)
                });
            }

            fs::write(html_dir.join(example).with_extension("html"), html)
                .unwrap_or_else(|_| panic!("Failed to create {}.html", example_str));
        }
    }

    examples_in.push_str("}");

    index_html.push_str("</ul></body></html>");

    // Write out the contents of the examples.in module.
    fs::write(src_dir.join("examples.in"), examples_in)?;

    // Write out the index.html file
    fs::write(crate_dir.join("index.html"), index_html)?;

    Ok(())
}
