use anyhow::{Context, Result, bail};
use index_core::*;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};
use walkdir::WalkDir;
fn run(program: &str, args: &[&str], root: &Path) -> Result<()> {
    let mut command = Command::new(program);
    command.args(args).current_dir(root);
    if program == "cargo" {
        // Panic locations in release binaries must not disclose the build user's paths.
        let mut flags = Vec::new();
        if !args.contains(&"wasm32-unknown-unknown") {
            flags.extend(["-C".to_string(), "target-feature=+crt-static".to_string()]);
        }
        if let Some(profile) = env::var_os("USERPROFILE") {
            flags.push(format!("--remap-path-prefix={}=/build/user", PathBuf::from(profile).display()));
        }
        flags.push(format!("--remap-path-prefix={}=/build/intelligence-index", root.display()));
        command.env("CARGO_ENCODED_RUSTFLAGS", flags.join("\u{1f}"));
    }
    let status = command.status()?;
    if !status.success() {
        bail!("{program} did not complete successfully");
    }
    Ok(())
}
fn capture(program: &str, args: &[&str], root: &Path) -> Result<Output> {
    let output = Command::new(program)
        .args(args)
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        bail!("{}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(output)
}
fn write(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> Result<()> {
    let path = path.as_ref();
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(path, bytes)?;
    Ok(())
}
fn json_file(path: &Path) -> Option<Value> {
    if path.is_file() {
        return fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    }
    let compressed = fs::File::open(path.with_extension("json.gz")).ok()?;
    serde_json::from_reader(flate2::read::GzDecoder::new(compressed)).ok()
}
fn copy(source: impl AsRef<Path>, destination: impl AsRef<Path>) -> Result<()> {
    let destination = destination.as_ref();
    fs::create_dir_all(destination.parent().unwrap())?;
    fs::copy(source, destination)?;
    Ok(())
}
fn gzip(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    encoder.write_all(bytes)?;
    write(path, encoder.finish()?)
}
fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for e in WalkDir::new(root) {
        let e = e?;
        if e.file_type().is_file() {
            files.push(e.into_path());
        }
    }
    files.sort();
    Ok(files)
}
fn remove_stage(path: &Path, parent: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let absolute = fs::canonicalize(path)?;
    let parent = fs::canonicalize(parent)?;
    if absolute.parent() != Some(parent.as_path())
        || !absolute
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(".Intelligence Index-")
    {
        bail!("Refusing to remove an unrelated directory");
    }
    fs::remove_dir_all(absolute)?;
    Ok(())
}
fn fonts(root: &Path, destination: &Path) -> Result<String> {
    let mut css = String::new();
    for file in files(&root.join("assets/fonts"))? {
        let name = file.file_name().unwrap().to_string_lossy();
        if !name.ends_with(".woff2") {
            continue;
        }
        let (family, weight) = if name.starts_with("dm-sans") {
            ("DM Sans Variable", "100 1000")
        } else if name.starts_with("dm-mono") {
            ("DM Mono", "400")
        } else {
            ("Instrument Serif", "400")
        };
        let style = if name.contains("italic") {
            "italic"
        } else {
            "normal"
        };
        let range = if name.contains("latin-ext") {
            "U+0100-02FF,U+0304,U+0308,U+0329,U+1D00-1DBF,U+1E00-1EFF,U+2020,U+20A0-20AB,U+20AD-20C0,U+2113,U+2C60-2C7F,U+A720-A7FF"
        } else {
            "U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+0304,U+0308,U+0329,U+2000-206F,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD"
        };
        css.push_str(&format!("@font-face{{font-family:'{family}';font-style:{style};font-display:swap;font-weight:{weight};src:url('/assets/fonts/{name}') format('woff2');unicode-range:{range}}}\n"));
        copy(&file, destination.join("assets/fonts").join(name.as_ref()))?;
    }
    write(destination.join("assets/fonts.css"), &css)?;
    Ok(css)
}
fn web(root: &Path, destination: &Path) -> Result<()> {
    run(
        "cargo",
        &[
            "build",
            "--locked",
            "--release",
            "-p",
            "index-ui",
            "--target",
            "wasm32-unknown-unknown",
        ],
        root,
    )?;
    run(
        "wasm-bindgen",
        &[
            "target/wasm32-unknown-unknown/release/index_ui.wasm",
            "--target",
            "web",
            "--no-typescript",
            "--omit-default-module-path",
            "--out-name",
            "app",
            "--out-dir",
            "target/web",
        ],
        root,
    )?;
    let wasm = fs::read(root.join("target/web/app_bg.wasm"))?;
    let wasm_name = format!("app-{}.wasm", &hash(&wasm)[..16]);
    gzip(&destination.join(format!("assets/{wasm_name}.gz")), &wasm)?;
    let js = fs::read(root.join("target/web/app.js"))?;
    let js_name = format!("app-{}.js", &hash(&js)[..16]);
    gzip(&destination.join(format!("assets/{js_name}.gz")), &js)?;
    fonts(root, destination)?;
    let mut css = fs::read_to_string(root.join("assets/theme.css"))?;
    css.push_str(
        &fs::read_to_string(root.join("assets/styles.css"))?
            .replace("@import url('./theme.css');", ""),
    );
    css.push_str(include_str!("../../../assets/rust-ui.css"));
    let css = Regex::new(r"(?s)/\*.*?\*/")?.replace_all(&css, "");
    let css = Regex::new(r"\s+")?.replace_all(&css, " ").into_owned();
    let css_name = format!("app-{}.css", &hash(css.as_bytes())[..16]);
    gzip(
        &destination.join(format!("assets/{css_name}.gz")),
        css.as_bytes(),
    )?;
    let html = format!(
        "<!doctype html><html lang=\"en\" data-theme=\"dark\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><meta name=\"theme-color\" content=\"#000000\"><title>Artificial Analysis · Intelligence Index</title><link rel=\"icon\" href=\"/logos/artificial-analysis.svg\" type=\"image/svg+xml\"><link rel=\"stylesheet\" href=\"/assets/fonts.css\"><link rel=\"stylesheet\" href=\"/assets/{css_name}\"><link rel=\"preload\" href=\"/assets/{wasm_name}\" as=\"fetch\" type=\"application/wasm\" crossorigin></head><body><div id=\"root\"></div><script type=\"module\">import init from '/assets/{js_name}';init({{module_or_path:'/assets/{wasm_name}'}});</script></body></html>"
    );
    write(destination.join("index.html"), html)?;
    Ok(())
}
fn decode(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}
fn licenses(root: &Path) -> Result<String> {
    let metadata: Value = serde_json::from_slice(
        &capture(
            "cargo",
            &["metadata", "--locked", "--format-version", "1"],
            root,
        )?
        .stdout,
    )?;
    let mut identities = BTreeSet::new();
    for (package, target) in [
        ("index-native", "x86_64-pc-windows-msvc"),
        ("index-ui", "wasm32-unknown-unknown"),
    ] {
        let output = capture(
            "cargo",
            &[
                "tree", "--locked", "-p", package, "--target", target, "--edges", "normal",
                "--prefix", "none", "--format", "{p}",
            ],
            root,
        )?;
        for line in String::from_utf8(output.stdout)?.lines() {
            let parts: Vec<_> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                identities.insert(format!("{}@{}", parts[0], parts[1].trim_start_matches('v')));
            }
        }
    }
    let mut notices: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut missing = Vec::new();
    for p in array(&metadata["packages"]) {
        let identity = format!("{}@{}", s(p, "name"), s(p, "version"));
        if !identities.contains(&identity) || p["source"].is_null() {
            continue;
        }
        let folder = Path::new(s(p, "manifest_path")).parent().unwrap();
        let mut count = 0;
        let mut paths = Vec::new();
        if let Some(file) = p["license_file"].as_str() {
            paths.push(folder.join(file));
        }
        for e in fs::read_dir(folder)? {
            let e = e?;
            let name = e.file_name().to_string_lossy().to_lowercase();
            if e.file_type()?.is_file()
                && Regex::new(r"^(?:licen[cs]e|notice|copying|copyright|ofl)(?:[._-]|$)")?
                    .is_match(&name)
            {
                paths.push(e.path());
            }
            if e.file_type()?.is_dir() && ["licenses", "licences"].contains(&name.as_str()) {
                paths.extend(files(&e.path())?);
            }
        }
        paths.sort();
        paths.dedup();
        for path in paths {
            let Ok(text) = fs::read_to_string(path) else {
                continue;
            };
            let text = text.trim().to_string();
            if text.len() > 50 {
                notices.entry(text).or_default().insert(identity.clone());
                count += 1;
            }
        }
        if count == 0
            && !root
                .join("assets/licenses")
                .join(format!("{}-{}.txt", s(p, "name"), s(p, "version")))
                .is_file()
        {
            missing.push(format!("{identity} ({})", s(p, "license")));
        }
    }
    if !missing.is_empty() {
        bail!(
            "Required dependency notices are missing for {}",
            missing.join(", ")
        );
    }
    for file in files(&root.join("assets/licenses"))? {
        notices
            .entry(fs::read_to_string(&file)?.trim().to_string())
            .or_default()
            .insert(file.file_stem().unwrap().to_string_lossy().to_string());
    }
    let sysroot = String::from_utf8(capture("rustc", &["--print", "sysroot"], root)?.stdout)?;
    let docs = Path::new(sysroot.trim()).join("share/doc/rust");
    let copyright = fs::read_to_string(docs.join("COPYRIGHT-library.html"))?;
    let pre = Regex::new(r"(?s)<pre>(.*?)</pre>")?;
    let mut library_notices = Vec::new();
    let reduced = pre.replace_all(&copyright, |capture: &regex::Captures| {
        let text = decode(capture[1].trim());
        let index = if let Some(i) = library_notices.iter().position(|v| *v == text) {
            i
        } else {
            library_notices.push(text);
            library_notices.len() - 1
        };
        format!("\n[Standard library notice {}]\n", index + 1)
    });
    let tags = Regex::new(r"<[^>]*>")?;
    let stripped = decode(&tags.replace_all(&reduced, ""));
    let lines: Vec<_> = stripped
        .lines()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .collect();
    let mut result = format!(
        "{}\n\n{}\n\nRust standard library attribution\n{}\n",
        fs::read_to_string(root.join("NOTICE"))?,
        fs::read_to_string(root.join("LICENSE"))?,
        lines.join("\n")
    );
    for (i, text) in library_notices.into_iter().enumerate() {
        notices
            .entry(text)
            .or_default()
            .insert(format!("Standard library notice {}", i + 1));
    }
    for file in [
        "Apache-2.0.txt",
        "MIT.txt",
        "LLVM-exception.txt",
        "Unicode-3.0.txt",
    ] {
        notices
            .entry(
                fs::read_to_string(docs.join("licenses").join(file))?
                    .trim()
                    .to_string(),
            )
            .or_default()
            .insert(format!("Rust standard library {file}"));
    }
    for (text, packages) in notices {
        result.push_str(&format!(
            "\n========================================\n{}\n\n{text}\n",
            packages.into_iter().collect::<Vec<_>>().join("\n")
        ));
    }
    Ok(result)
}
fn saved_data(root: &Path, installed: &Path, stage: &Path) -> Result<(usize, usize)> {
    let mut candidates: Vec<_> = [root, installed]
        .iter()
        .filter_map(|p| json_file(&p.join("public/models.json")).map(|v| (*p, v)))
        .collect();
    candidates.sort_by(|a, b| s(&b.1, "updatedAt").cmp(s(&a.1, "updatedAt")));
    let (source, mut snapshot) = candidates
        .into_iter()
        .find(|(_, v)| v["models"].is_array())
        .context("A saved model snapshot is required")?;
    let mut raw = json_file(&source.join("data/public-models.raw.json"))
        .context("The source export is required")?;
    retain(&mut snapshot, &mut raw);
    let ids: HashSet<_> = array(&snapshot["models"])
        .iter()
        .map(|m| s(m, "id"))
        .collect();
    gzip(
        &stage.join("public/models.json.gz"),
        &serde_json::to_vec(&snapshot)?,
    )?;
    gzip(
        &stage.join("data/public-models.raw.json.gz"),
        &serde_json::to_vec(&raw)?,
    )?;
    let mut count = 0;
    for parent in [installed, root] {
        if let Some(selection) = json_file(&parent.join("data/model-selection.json")) {
            if selection["selected"].is_array() {
                let mut seen = HashSet::new();
                let selected: Vec<_> = array(&selection["selected"])
                    .iter()
                    .filter(|v| {
                        v.as_str()
                            .is_some_and(|id| ids.contains(id) && seen.insert(id))
                    })
                    .cloned()
                    .collect();
                count = selected.len();
                write(
                    stage.join("data/model-selection.json"),
                    serde_json::to_vec(&json!({"selected":selected}))?,
                )?;
                break;
            }
        }
    }
    let mut logos: BTreeSet<String> = array(&snapshot["models"])
        .iter()
        .filter_map(|m| {
            Path::new(s(m, "logo"))
                .file_name()
                .map(|v| v.to_string_lossy().to_string())
        })
        .collect();
    logos.insert("artificial-analysis.svg".into());
    let mut content: BTreeMap<String, String> = BTreeMap::new();
    let mut replacements = BTreeMap::new();
    for name in logos {
        let original = [source, installed, root]
            .iter()
            .map(|p| p.join("public/logos").join(&name))
            .find(|p| p.is_file());
        if let Some(original) = original {
            let bytes = fs::read(&original)?;
            let identity = hash(&bytes);
            if let Some(existing) = content.get(&identity) {
                replacements.insert(name, existing.clone());
            } else {
                copy(&original, stage.join("public/logos").join(&name))?;
                content.insert(identity, name);
            }
        }
    }
    if !replacements.is_empty() {
        for model in snapshot["models"].as_array_mut().unwrap() {
            let name = Path::new(s(model, "logo"))
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            if let Some(existing) = replacements.get(name.as_ref()) {
                model["logo"] = json!(format!("/logos/{existing}"));
            }
        }
        gzip(
            &stage.join("public/models.json.gz"),
            &serde_json::to_vec(&snapshot)?,
        )?;
    }
    Ok((array(&snapshot["models"]).len(), count))
}
fn build(root: &Path) -> Result<()> {
    run(
        "cargo",
        &["build", "--locked", "--release", "-p", "index-native"],
        root,
    )?;
    let documents = PathBuf::from(env::var_os("USERPROFILE").context("User profile unavailable")?)
        .join("Documents");
    let installed = documents.join("Intelligence Index");
    if installed.exists()
        && (!installed.join("Intelligence Index.exe").is_file()
            || !installed.join("desktop/build-version.txt").is_file()
            || (!installed.join("public/models.json").is_file()
                && !installed.join("public/models.json.gz").is_file()))
    {
        bail!("The destination contains unrelated files");
    }
    let identity = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let stage = documents.join(format!(".Intelligence Index-build-{identity}"));
    let backup = documents.join(format!(".Intelligence Index-previous-{identity}"));
    fs::create_dir_all(&stage)?;
    let result = (|| -> Result<()> {
        web(root, &stage.join("dist"))?;
        copy(
            root.join("target/release/intelligence-index.exe"),
            stage.join("Intelligence Index.exe"),
        )?;
        write(stage.join("LICENSES.txt"), licenses(root)?)?;
        if installed.exists() {
            let mut command = Command::new(stage.join("Intelligence Index.exe"));
            command
                .args(["--root"])
                .arg(&installed)
                .arg("--stop-server");
            if !command.status()?.success() {
                bail!("The installed application could not finish its active writes");
            }
        }
        let (models, selections) = saved_data(root, &installed, &stage)?;
        let mut hasher = Sha256::new();
        hasher.update(b"rust-wasm\0win-x64\0");
        for path in files(&stage)? {
            if path.starts_with(stage.join("data")) || path.starts_with(stage.join("public")) {
                continue;
            }
            hasher.update(
                path.strip_prefix(&stage)?
                    .to_string_lossy()
                    .replace('\\', "/")
                    .as_bytes(),
            );
            hasher.update([0]);
            hasher.update(fs::read(&path)?);
            hasher.update([0]);
        }
        let version = hex::encode(hasher.finalize());
        write(stage.join("desktop/build-version.txt"), &version)?;
        if installed.exists() {
            fs::rename(&installed, &backup)?;
        }
        if let Err(e) = fs::rename(&stage, &installed) {
            if backup.exists() {
                fs::rename(&backup, &installed)?;
            }
            return Err(e.into());
        }
        write(root.join("desktop/build-version.txt"), &version)?;
        let launcher = installed.join("Intelligence Index.exe");
        copy(&launcher, documents.join("Intelligence Index.exe"))?;
        copy(&launcher, root.join("Intelligence Index.exe"))?;
        let desktop = documents
            .parent()
            .unwrap()
            .join("Desktop/Intelligence Index.exe");
        if desktop.is_file() {
            copy(&launcher, desktop)?;
        }
        let output = Command::new(documents.join("Intelligence Index.exe"))
            .arg("--ensure-server")
            .output()?;
        if !output.status.success() {
            bail!(
                "The installed build could not activate {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        remove_stage(&backup, &documents)?;
        let list = files(&installed)?;
        let bytes = list
            .iter()
            .map(|p| fs::metadata(p).map(|m| m.len()))
            .collect::<std::io::Result<Vec<_>>>()?
            .iter()
            .sum::<u64>();
        println!(
            "Installed Rust production build {version}\n{}\n{models} eligible models, {selections} saved selections\n{} files, {:.2} MiB",
            installed.display(),
            list.len(),
            bytes as f64 / 1048576.
        );
        Ok(())
    })();
    if stage.exists() {
        remove_stage(&stage, &documents)?;
    }
    result
}
fn main() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    match env::args().nth(1).as_deref() {
        Some("build") => build(&root),
        Some("web") => {
            let destination = PathBuf::from(
                env::args()
                    .nth(2)
                    .context("A web output directory is required")?,
            );
            web(&root, &destination)
        }
        _ => bail!("Use cargo run --release -p xtask -- build"),
    }
}
