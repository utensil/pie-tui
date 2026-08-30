//! xt — repo tasks. Boundary gate enforced per AGENTS.md §Boundary rules.
//!
//! Layer model (rank strictly increases downward):
//!   0 pie-core · 1 pie-term · 2 pie-components · 3 pie-app · 4 adapters/* · 5 xt
//! Rules:
//!   D1 A crate may depend only on workspace crates of strictly LOWER rank.
//!   D2 Nothing may depend on an adapter except another adapter (implied by D1, asserted
//!      explicitly for readable errors).
//!   S1 Sources under pie-core must never reference sibling crate names (pure core).
//!   S2 No host absolute paths (slash-Users or slash-home prefixes) in any committed
//!      source file. The gate scans itself too, so its own literals are runtime-assembled.
//!   S3 The gate must stay dependency-free.
//!
//! Usage: cargo run -p xt -- boundary

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CrateInfo {
    pub rank: u8,
    pub dir: &'static str,
}

pub type Manifests = BTreeMap<String, String>;
pub type Sources = BTreeMap<String, Vec<(PathBuf, String)>>;

/// First path segment of the banned prefix, assembled at runtime so this source never
/// contains the literal it bans (S2 scans every committed file, including this one).
fn host_users_segment() -> String {
    ["U", "sers"].concat()
}

fn host_path_prefixes() -> Vec<String> {
    vec![
        format!("/{}/", host_users_segment()),
        format!("/{}", "home"),
    ]
}

fn known_crates() -> BTreeMap<String, CrateInfo> {
    let mut m = BTreeMap::new();
    m.insert(
        "pie-core".into(),
        CrateInfo {
            rank: 0,
            dir: "crates/pie-core",
        },
    );
    m.insert(
        "pie-term".into(),
        CrateInfo {
            rank: 1,
            dir: "crates/pie-term",
        },
    );
    m.insert(
        "pie-components".into(),
        CrateInfo {
            rank: 2,
            dir: "crates/pie-components",
        },
    );
    m.insert(
        "pie-app".into(),
        CrateInfo {
            rank: 3,
            dir: "crates/pie-app",
        },
    );
    m.insert(
        "pie-napi".into(),
        CrateInfo {
            rank: 4,
            dir: "adapters/pie-napi",
        },
    );
    m.insert(
        "xt".into(),
        CrateInfo {
            rank: 5,
            dir: "xtask",
        },
    );
    m
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DependencySpec {
    alias: String,
    package: Option<String>,
    workspace: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DependencySection {
    None,
    Listing,
    Detail(String),
}

fn unquote_toml_key(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        value[1..value.len() - 1].to_string()
    } else {
        value.to_string()
    }
}

fn parse_toml_string(value: &str) -> Option<String> {
    let value = value.trim();
    let quote = value.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = &value[quote.len_utf8()..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

fn dependency_section(header: &str, workspace_table: bool) -> DependencySection {
    let header = header.trim();
    if workspace_table {
        return if header == "workspace.dependencies" {
            DependencySection::Listing
        } else if let Some(alias) = header.strip_prefix("workspace.dependencies.") {
            DependencySection::Detail(unquote_toml_key(alias))
        } else {
            DependencySection::None
        };
    }
    if header.starts_with("workspace.") {
        return DependencySection::None;
    }
    for kind in ["dependencies", "dev-dependencies", "build-dependencies"] {
        let target_table = header.starts_with("target.");
        if header == kind || (target_table && header.ends_with(&format!(".{kind}"))) {
            return DependencySection::Listing;
        }
        let marker = format!(".{kind}.");
        if target_table && let Some((_, alias)) = header.rsplit_once(&marker) {
            return DependencySection::Detail(unquote_toml_key(alias));
        }
        let prefix = format!("{kind}.");
        if let Some(alias) = header.strip_prefix(&prefix) {
            return DependencySection::Detail(unquote_toml_key(alias));
        }
    }
    DependencySection::None
}

fn inline_field(value: &str, wanted: &str) -> Option<String> {
    let value = value.trim().trim_start_matches('{').trim_end_matches('}');
    value.split(',').find_map(|field| {
        let (key, value) = field.split_once('=')?;
        (unquote_toml_key(key) == wanted).then(|| value.trim().to_string())
    })
}

/// Extract dependency declarations from every Cargo dependency table form we use:
/// normal/dev/build, target-qualified variants, detailed subtables, renamed packages,
/// and dotted `alias.workspace = true` inheritance. This intentionally remains a
/// dependency-free, bounded Cargo-manifest parser rather than a general TOML parser.
fn dependency_specs(manifest: &str, workspace_table: bool) -> Vec<DependencySpec> {
    let mut section = DependencySection::None;
    let mut detail: Option<DependencySpec> = None;
    let mut specs = Vec::new();

    for line in manifest.lines() {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            if let Some(spec) = detail.take() {
                specs.push(spec);
            }
            section = dependency_section(&t[1..t.len() - 1], workspace_table);
            if let DependencySection::Detail(alias) = &section {
                detail = Some(DependencySpec {
                    alias: alias.clone(),
                    package: None,
                    workspace: false,
                });
            }
            continue;
        }
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let Some((raw_key, raw_value)) = t.split_once('=') else {
            continue;
        };
        match &section {
            DependencySection::Listing => {
                let mut key = unquote_toml_key(raw_key);
                let dotted_workspace = key.ends_with(".workspace");
                if dotted_workspace {
                    key.truncate(key.len() - ".workspace".len());
                }
                let package =
                    inline_field(raw_value, "package").and_then(|value| parse_toml_string(&value));
                let workspace = dotted_workspace
                    || inline_field(raw_value, "workspace").as_deref() == Some("true");
                specs.push(DependencySpec {
                    alias: unquote_toml_key(&key),
                    package,
                    workspace,
                });
            }
            DependencySection::Detail(_) => {
                let Some(spec) = detail.as_mut() else {
                    continue;
                };
                match unquote_toml_key(raw_key).as_str() {
                    "package" => spec.package = parse_toml_string(raw_value),
                    "workspace" => spec.workspace = raw_value.trim() == "true",
                    _ => {}
                }
            }
            DependencySection::None => {}
        }
    }
    if let Some(spec) = detail {
        specs.push(spec);
    }
    specs
}

fn workspace_dependency_packages(manifest: Option<&String>) -> BTreeMap<String, String> {
    manifest
        .map(|text| {
            dependency_specs(text, true)
                .into_iter()
                .map(|spec| {
                    let package = spec.package.unwrap_or_else(|| spec.alias.clone());
                    (spec.alias, package)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn dependency_names(manifest: &str, workspace_packages: &BTreeMap<String, String>) -> Vec<String> {
    dependency_specs(manifest, false)
        .into_iter()
        .map(|spec| {
            if spec.workspace {
                workspace_packages
                    .get(&spec.alias)
                    .cloned()
                    .or(spec.package)
                    .unwrap_or(spec.alias)
            } else {
                spec.package.unwrap_or(spec.alias)
            }
        })
        .collect()
}

/// Load the working-tree contents of every Git-tracked path. Invalid UTF-8 or NUL-bearing
/// blobs are treated as binary and counted, not decoded. There is deliberately no
/// extension/path allowlist: tests, fixtures, scripts, and top-level files are all scanned.
fn collect_tracked_text_files(repo: &Path) -> Result<(Vec<(PathBuf, String)>, usize), String> {
    let output = Command::new("git")
        .args(["ls-files", "-z", "--cached"])
        .current_dir(repo)
        .output()
        .map_err(|error| format!("cannot run git ls-files: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git ls-files failed with {}",
            output.status.code().unwrap_or(1)
        ));
    }
    let paths = String::from_utf8(output.stdout)
        .map_err(|_| "git ls-files returned a non-UTF-8 path".to_string())?;
    let mut text_files = Vec::new();
    let mut binary_files = 0;
    for path in paths.split('\0').filter(|path| !path.is_empty()) {
        let relative = PathBuf::from(path);
        if relative.is_absolute()
            || relative
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err(format!("git reported unsafe tracked path {path:?}"));
        }
        let absolute = repo.join(&relative);
        let metadata = std::fs::symlink_metadata(&absolute)
            .map_err(|error| format!("cannot inspect {path}: {error}"))?;
        let bytes = if metadata.file_type().is_symlink() {
            std::fs::read_link(&absolute)
                .map_err(|error| format!("cannot read symlink {path}: {error}"))?
                .to_string_lossy()
                .into_owned()
                .into_bytes()
        } else if metadata.is_file() {
            std::fs::read(&absolute).map_err(|error| format!("cannot read {path}: {error}"))?
        } else {
            binary_files += 1;
            continue;
        };
        if bytes.contains(&0) {
            binary_files += 1;
            continue;
        }
        match String::from_utf8(bytes) {
            Ok(content) => text_files.push((relative, content)),
            Err(_) => binary_files += 1,
        }
    }
    text_files.sort_by(|left, right| left.0.cmp(&right.0));
    Ok((text_files, binary_files))
}

#[derive(Debug)]
pub struct Violation {
    pub rule: &'static str,
    pub message: String,
}

/// Pure checker over crate metadata, manifests, and source texts. Filesystem-free so it
/// can be unit-tested on synthetic inputs (the live runner feeds real files).
pub fn check_boundary(
    known: &BTreeMap<String, CrateInfo>,
    manifests: &Manifests,
    sources: &Sources,
) -> Vec<Violation> {
    let mut v = Vec::new();
    let workspace_packages = workspace_dependency_packages(manifests.get("workspace"));

    // S3: the gate itself stays dependency-free.
    if let Some(xt_manifest) = manifests.get("xt")
        && !dependency_names(xt_manifest, &workspace_packages).is_empty()
    {
        {
            v.push(Violation {
                rule: "S3",
                message: "xtask must remain dependency-free (found a Cargo dependency entry)"
                    .into(),
            });
        }
    }

    // D1/D2: depend only downward.
    for (name, info) in known {
        let Some(deps_text) = manifests.get(name) else {
            continue;
        };
        for dep in dependency_names(deps_text, &workspace_packages) {
            if dep == *name {
                continue;
            }
            let Some(dep_info) = known.get(&dep) else {
                continue; // third-party dep: layer ranks don't apply
            };
            if dep_info.rank >= info.rank {
                v.push(Violation {
                    rule: "D1",
                    message: format!(
                        "{name} (rank {}) must not depend on `{dep}` (rank {})",
                        info.rank, dep_info.rank
                    ),
                });
            }
        }
    }

    // S1: every tracked file below pie-core must not name siblings (pure core).
    let banned = ["pie_term", "pie_components", "pie_app", "pie_napi"];
    for files in sources.values() {
        for (path, content) in files {
            if !path.starts_with("crates/pie-core") {
                continue;
            }
            for b in banned {
                if content.contains(b) {
                    v.push(Violation {
                        rule: "S1",
                        message: format!(
                            "{} references `{b}`; pie-core must be pure",
                            path.display()
                        ),
                    });
                }
            }
        }
    }

    // S2: no host absolute paths in any tracked text artifact (self included).
    for files in sources.values() {
        for (path, content) in files {
            for prefix in host_path_prefixes() {
                let documented_placeholder = [prefix.as_str(), "<", "user", ">"].concat();
                let scan = if path == Path::new("AGENTS.md") {
                    content.replace(&documented_placeholder, "")
                } else {
                    content.clone()
                };
                if scan.contains(prefix.as_str()) {
                    v.push(Violation {
                        rule: "S2",
                        message: format!(
                            "{} contains host absolute path ({prefix})",
                            path.display()
                        ),
                    });
                }
            }
        }
    }

    v
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("boundary") => {
            let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .to_path_buf();
            let known = known_crates();
            let mut manifests = Manifests::new();
            let mut sources = Sources::new();
            match std::fs::read_to_string(repo.join("Cargo.toml")) {
                Ok(text) => {
                    manifests.insert("workspace".to_string(), text);
                }
                Err(error) => {
                    eprintln!("boundary: cannot read workspace Cargo.toml: {error}");
                    std::process::exit(2);
                }
            }
            for (name, info) in &known {
                let manifest_path = repo.join(info.dir).join("Cargo.toml");
                match std::fs::read_to_string(&manifest_path) {
                    Ok(text) => {
                        manifests.insert(name.clone(), text);
                    }
                    Err(e) => {
                        eprintln!("boundary: cannot read {}: {e}", manifest_path.display());
                        std::process::exit(2);
                    }
                }
            }
            let (tracked, binary_count) = match collect_tracked_text_files(&repo) {
                Ok(result) => result,
                Err(error) => {
                    eprintln!("boundary: {error}");
                    std::process::exit(2);
                }
            };
            let text_count = tracked.len();
            sources.insert("tracked".to_string(), tracked);
            let violations = check_boundary(&known, &manifests, &sources);
            if violations.is_empty() {
                println!(
                    "boundary: OK ({} crates, {text_count} tracked text, {binary_count} binary/non-file checked)",
                    known.len()
                );
            } else {
                for violation in &violations {
                    eprintln!("[{}] {}", violation.rule, violation.message);
                }
                eprintln!("boundary: {} violation(s)", violations.len());
                std::process::exit(1);
            }
        }
        _ => {
            eprintln!("usage: cargo run -p xt -- boundary");
            std::process::exit(64);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> (BTreeMap<String, CrateInfo>, Manifests, Sources) {
        (
            known_crates(),
            BTreeMap::from([
                ("xt".to_string(), "[package]\nname = \"xt\"\n".to_string()),
                ("pie-core".to_string(), "[dependencies]\n".to_string()),
                (
                    "pie-term".to_string(),
                    "[dependencies]\npie-core = { path = \"../pie-core\" }\n".to_string(),
                ),
                (
                    "pie-components".to_string(),
                    "[dependencies]\npie-core = { path = \"../../crates/pie-core\" }\npie-term = { path = \"../../crates/pie-term\" }\n"
                        .to_string(),
                ),
            ]),
            BTreeMap::new(),
        )
    }

    #[test]
    fn clean_layout_passes() {
        let (k, m, s) = base();
        assert!(check_boundary(&k, &m, &s).is_empty());
    }

    #[test]
    fn upward_dependency_violates_d1() {
        let (k, mut m, s) = base();
        let core = m.get_mut("pie-core").unwrap();
        core.push_str("pie-term = { path = \"../pie-term\" }\n");
        let v = check_boundary(&k, &m, &s);
        assert_eq!(v.len(), 1, "core-to-term must violate D1");
        assert_eq!(v[0].rule, "D1");
    }

    #[test]
    fn app_cannot_depend_on_adapter() {
        let (k, mut m, s) = base();
        m.insert(
            "pie-app".to_string(),
            "[dependencies]\npie-napi = { path = \"../../adapters/pie-napi\" }\n".to_string(),
        );
        let v = check_boundary(&k, &m, &s);
        assert!(
            v.iter()
                .any(|x| x.rule == "D1" && x.message.contains("pie-napi"))
        );
    }

    #[test]
    fn core_sources_must_stay_pure() {
        let (k, m, mut s) = base();
        s.insert(
            "pie-core".to_string(),
            vec![(
                PathBuf::from("crates/pie-core/src/lib.rs"),
                "use pie_term::foo;\n".into(),
            )],
        );
        let v = check_boundary(&k, &m, &s);
        assert!(v.iter().any(|x| x.rule == "S1"));
    }

    #[test]
    fn no_host_paths_anywhere() {
        let (k, m, mut s) = base();
        // Built at runtime so THIS file never carries a banned literal.
        let fixture_line = format!(
            "let p = \"/{seg}/{rest}\";\n",
            seg = host_users_segment(),
            rest = "utensil"
        );
        s.insert(
            "pie-app".to_string(),
            vec![(PathBuf::from("crates/pie-app/src/main.rs"), fixture_line)],
        );
        let v = check_boundary(&k, &m, &s);
        assert!(
            v.iter().any(|x| x.rule == "S2"),
            "expected S2 hit, got {v:?}"
        );
    }

    #[test]
    fn xtask_must_stay_depfree() {
        let (k, mut m, s) = base();
        m.insert(
            "xt".to_string(),
            concat!("[dependencies]", "\nserde = ", "\"1\"\n").to_string(),
        );
        let v = check_boundary(&k, &m, &s);
        assert!(v.iter().any(|x| x.rule == "S3"));
    }

    #[test]
    fn every_cargo_dependency_kind_enforces_layering() {
        for table in [
            "dev-dependencies",
            "build-dependencies",
            "target.'cfg(unix)'.dependencies",
            "target.'cfg(unix)'.dev-dependencies",
            "target.'cfg(unix)'.build-dependencies",
        ] {
            let (k, mut m, s) = base();
            m.insert(
                "pie-core".to_string(),
                format!("[{table}]\npie-term = {{ path = \"../pie-term\" }}\n"),
            );
            let v = check_boundary(&k, &m, &s);
            assert!(
                v.iter().any(|x| x.rule == "D1"),
                "{table} escaped layering: {v:?}"
            );
        }
    }

    #[test]
    fn renamed_and_workspace_dependencies_resolve_real_package() {
        let (k, mut m, s) = base();
        m.insert(
            "workspace".to_string(),
            "[workspace.dependencies]\nterm-alias = { package = \"pie-term\", path = \"crates/pie-term\" }\n"
                .to_string(),
        );
        for core_manifest in [
            "[dependencies]\nterm-alias = { package = \"pie-term\", path = \"../pie-term\" }\n",
            "[dependencies.term-alias]\npackage = \"pie-term\"\npath = \"../pie-term\"\n",
            "[target.'cfg(unix)'.dev-dependencies]\nterm-alias.workspace = true\n",
        ] {
            m.insert("pie-core".to_string(), core_manifest.to_string());
            let v = check_boundary(&k, &m, &s);
            assert!(
                v.iter().any(|x| x.rule == "D1"),
                "renamed dependency escaped layering for {core_manifest:?}: {v:?}"
            );
        }
    }

    #[test]
    fn xtask_must_stay_depfree_in_every_dependency_kind() {
        for table in ["dev-dependencies", "build-dependencies"] {
            let (k, mut m, s) = base();
            m.insert("xt".to_string(), format!("[{table}]\nserde = \"1\"\n"));
            let v = check_boundary(&k, &m, &s);
            assert!(
                v.iter().any(|x| x.rule == "S3"),
                "xtask {table} escaped S3: {v:?}"
            );
        }
    }
}
