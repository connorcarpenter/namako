use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use serde::Deserialize;
use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

const MAX_SKILL_NAME_LEN: usize = 64;
const MAX_DESC_LEN: usize = 1024;

// ─── CLI ────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "skill-tool", about = "Codex skill management CLI")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create a new skill directory with a SKILL.md template
    Init {
        /// Skill name (normalized to hyphen-case)
        skill_name: String,
        /// Output directory for the skill
        #[arg(long)]
        path: String,
        /// Comma-separated resource dirs: scripts,references,assets
        #[arg(long, default_value = "")]
        resources: String,
        /// Create example files inside selected resource directories
        #[arg(long)]
        examples: bool,
    },
    /// Validate a skill directory
    Validate {
        /// Path to skill directory
        skill_path: String,
    },
    /// Package a skill directory into a distributable .skill file
    Package {
        /// Path to skill folder
        skill_path: String,
        /// Output directory (defaults to current directory)
        output_dir: Option<String>,
    },
    /// Install a skill from GitHub
    Install {
        /// GitHub URL (https://github.com/owner/repo[/tree/ref/path])
        #[arg(long)]
        url: Option<String>,
        /// Repository in owner/repo format
        #[arg(long)]
        repo: Option<String>,
        /// Path(s) to skill(s) inside repo
        #[arg(long, num_args = 1..)]
        skill_path: Option<Vec<String>>,
        /// Git ref (branch/tag/commit)
        #[arg(long, default_value = "main")]
        ref_: String,
        /// Destination skills directory (defaults to $CODEX_HOME/skills)
        #[arg(long)]
        dest: Option<String>,
        /// Destination skill name (defaults to basename of path)
        #[arg(long)]
        name: Option<String>,
        /// Download method: auto, download, git
        #[arg(long, default_value = "auto")]
        method: String,
    },
    /// List curated skills from a GitHub repo
    List {
        /// Repository in owner/repo format
        #[arg(long, default_value = "openai/skills")]
        repo: String,
        /// Path inside repo containing curated skills
        #[arg(long, default_value = "skills/.curated")]
        skill_path: String,
        /// Git ref
        #[arg(long, default_value = "main")]
        ref_: String,
        /// Output format: text or json
        #[arg(long, default_value = "text")]
        format: String,
    },
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.cmd {
        Cmd::Init {
            skill_name,
            path,
            resources,
            examples,
        } => cmd_init(&skill_name, &path, &resources, examples),
        Cmd::Validate { skill_path } => cmd_validate(&skill_path),
        Cmd::Package {
            skill_path,
            output_dir,
        } => cmd_package(&skill_path, output_dir.as_deref()),
        Cmd::Install {
            url,
            repo,
            skill_path,
            ref_,
            dest,
            name,
            method,
        } => cmd_install(
            url.as_deref(),
            repo.as_deref(),
            skill_path.as_deref(),
            &ref_,
            dest.as_deref(),
            name.as_deref(),
            &method,
        ),
        Cmd::List {
            repo,
            skill_path,
            ref_,
            format,
        } => cmd_list(&repo, &skill_path, &ref_, &format),
    };
    if let Err(e) = result {
        eprintln!("[ERROR] {e}");
        std::process::exit(1);
    }
}

// ─── INIT ───────────────────────────────────────────────────────────────────

fn normalize_skill_name(raw: &str) -> String {
    let lower = raw.trim().to_lowercase();
    let normalized: String = lower
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let normalized = normalized.trim_matches('-').to_string();
    // collapse consecutive hyphens
    let mut out = String::new();
    let mut prev_hyphen = false;
    for c in normalized.chars() {
        if c == '-' {
            if !prev_hyphen {
                out.push(c);
            }
            prev_hyphen = true;
        } else {
            out.push(c);
            prev_hyphen = false;
        }
    }
    out
}

fn title_case(name: &str) -> String {
    name.split('-')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_resources(raw: &str) -> Result<Vec<&str>> {
    if raw.is_empty() {
        return Ok(vec![]);
    }
    let allowed = ["scripts", "references", "assets"];
    let mut seen = std::collections::HashSet::new();
    let mut out = vec![];
    for item in raw.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if !allowed.contains(&item) {
            bail!(
                "Unknown resource type '{}'. Allowed: scripts, references, assets",
                item
            );
        }
        if seen.insert(item) {
            out.push(item);
        }
    }
    Ok(out)
}

const SKILL_TEMPLATE: &str = r#"---
name: {SKILL_NAME}
description: [TODO: Complete and informative explanation of what the skill does and when to use it. Include WHEN to use this skill - specific scenarios, file types, or tasks that trigger it.]
---

# {SKILL_TITLE}

## Overview

[TODO: 1-2 sentences explaining what this skill enables]

## Structuring This Skill

[TODO: Choose the structure that best fits this skill's purpose. Common patterns:

**1. Workflow-Based** (best for sequential processes)
**2. Task-Based** (best for tool collections)
**3. Reference/Guidelines** (best for standards or specifications)
**4. Capabilities-Based** (best for integrated systems)

Delete this entire "Structuring This Skill" section when done - it's just guidance.]

## [TODO: Replace with the first main section based on chosen structure]

[TODO: Add content here.]
"#;

const EXAMPLE_SCRIPT: &str = r#"#!/usr/bin/env bash
# Example helper script for {SKILL_NAME}
# Replace with actual implementation or delete if not needed.
echo "This is an example script for {SKILL_NAME}"
"#;

const EXAMPLE_REFERENCE: &str = r#"# Reference Documentation for {SKILL_TITLE}

This is a placeholder for detailed reference documentation.
Replace with actual reference content or delete if not needed.
"#;

const EXAMPLE_ASSET: &str = "Example asset placeholder. Replace with actual asset files.\n";

fn cmd_init(raw_name: &str, path: &str, resources: &str, examples: bool) -> Result<()> {
    let skill_name = normalize_skill_name(raw_name);
    if skill_name.is_empty() {
        bail!("Skill name must include at least one letter or digit.");
    }
    if skill_name.len() > MAX_SKILL_NAME_LEN {
        bail!(
            "Skill name '{}' is too long ({} chars, max {}).",
            skill_name,
            skill_name.len(),
            MAX_SKILL_NAME_LEN
        );
    }
    if skill_name != raw_name {
        println!(
            "Note: Normalized skill name from '{}' to '{}'.",
            raw_name, skill_name
        );
    }

    let resource_list = parse_resources(resources)?;
    if examples && resource_list.is_empty() {
        bail!("--examples requires --resources to be set.");
    }

    let skill_dir = Path::new(path).join(&skill_name);
    if skill_dir.exists() {
        bail!("Skill directory already exists: {}", skill_dir.display());
    }
    fs::create_dir_all(&skill_dir)
        .with_context(|| format!("Creating skill dir: {}", skill_dir.display()))?;
    println!("[OK] Created skill directory: {}", skill_dir.display());

    let skill_title = title_case(&skill_name);
    let skill_md = SKILL_TEMPLATE
        .replace("{SKILL_NAME}", &skill_name)
        .replace("{SKILL_TITLE}", &skill_title);
    fs::write(skill_dir.join("SKILL.md"), &skill_md)?;
    println!("[OK] Created SKILL.md");

    for res in &resource_list {
        let res_dir = skill_dir.join(res);
        fs::create_dir_all(&res_dir)?;
        match *res {
            "scripts" => {
                if examples {
                    let content = EXAMPLE_SCRIPT.replace("{SKILL_NAME}", &skill_name);
                    let p = res_dir.join("example.sh");
                    fs::write(&p, &content)?;
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        fs::set_permissions(&p, fs::Permissions::from_mode(0o755))?;
                    }
                    println!("[OK] Created scripts/example.sh");
                } else {
                    println!("[OK] Created scripts/");
                }
            }
            "references" => {
                if examples {
                    let content = EXAMPLE_REFERENCE.replace("{SKILL_TITLE}", &skill_title);
                    fs::write(res_dir.join("api_reference.md"), &content)?;
                    println!("[OK] Created references/api_reference.md");
                } else {
                    println!("[OK] Created references/");
                }
            }
            "assets" => {
                if examples {
                    fs::write(res_dir.join("example_asset.txt"), EXAMPLE_ASSET)?;
                    println!("[OK] Created assets/example_asset.txt");
                } else {
                    println!("[OK] Created assets/");
                }
            }
            _ => {}
        }
    }

    println!(
        "\n[OK] Skill '{}' initialized successfully at {}",
        skill_name,
        skill_dir.display()
    );
    println!("\nNext steps:");
    println!("1. Edit SKILL.md to complete the TODO items and update the description");
    if !resource_list.is_empty() {
        println!("2. Add resources to scripts/, references/, assets/ as needed");
    }
    println!("3. Run `skill-tool validate` when ready to check the skill structure");
    Ok(())
}

// ─── VALIDATE ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct Frontmatter {
    name: Option<serde_yaml::Value>,
    description: Option<serde_yaml::Value>,
    #[serde(flatten)]
    extra: HashMap<String, serde_yaml::Value>,
}

fn validate_skill(skill_path: &Path) -> Result<()> {
    let skill_md = skill_path.join("SKILL.md");
    if !skill_md.exists() {
        bail!("SKILL.md not found");
    }
    let content = fs::read_to_string(&skill_md)?;
    if !content.starts_with("---") {
        bail!("No YAML frontmatter found");
    }
    let end = content[3..]
        .find("\n---")
        .ok_or_else(|| anyhow::anyhow!("Invalid frontmatter format"))?;
    let fm_text = &content[3..3 + end + 1]; // include leading newline
    let fm: Frontmatter =
        serde_yaml::from_str(fm_text).with_context(|| "Invalid YAML in frontmatter")?;

    let allowed = [
        "name",
        "description",
        "license",
        "allowed-tools",
        "metadata",
    ];
    for key in fm.extra.keys() {
        if !allowed.contains(&key.as_str()) {
            bail!(
                "Unexpected key '{}' in frontmatter. Allowed: {}",
                key,
                allowed.join(", ")
            );
        }
    }

    let name = match &fm.name {
        None => bail!("Missing 'name' in frontmatter"),
        Some(serde_yaml::Value::String(s)) => s.trim().to_string(),
        Some(other) => bail!("'name' must be a string, got {:?}", other),
    };
    let desc = match &fm.description {
        None => bail!("Missing 'description' in frontmatter"),
        Some(serde_yaml::Value::String(s)) => s.trim().to_string(),
        Some(other) => bail!("'description' must be a string, got {:?}", other),
    };

    if !name.is_empty() {
        if !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            bail!(
                "Name '{}' must be hyphen-case (lowercase, digits, hyphens only)",
                name
            );
        }
        if name.starts_with('-') || name.ends_with('-') || name.contains("--") {
            bail!(
                "Name '{}' cannot start/end with hyphens or contain consecutive hyphens",
                name
            );
        }
        if name.len() > MAX_SKILL_NAME_LEN {
            bail!(
                "Name is too long ({} chars, max {})",
                name.len(),
                MAX_SKILL_NAME_LEN
            );
        }
    }

    if !desc.is_empty() {
        if desc.contains('<') || desc.contains('>') {
            bail!("Description cannot contain angle brackets");
        }
        if desc.len() > MAX_DESC_LEN {
            bail!(
                "Description is too long ({} chars, max {})",
                desc.len(),
                MAX_DESC_LEN
            );
        }
    }

    Ok(())
}

fn cmd_validate(skill_path: &str) -> Result<()> {
    let p = Path::new(skill_path);
    validate_skill(p)?;
    println!("[OK] Skill is valid!");
    Ok(())
}

// ─── PACKAGE ────────────────────────────────────────────────────────────────

fn cmd_package(skill_path: &str, output_dir: Option<&str>) -> Result<()> {
    let skill_path = Path::new(skill_path)
        .canonicalize()
        .with_context(|| format!("Skill path not found: {skill_path}"))?;
    if !skill_path.is_dir() {
        bail!("Path is not a directory: {}", skill_path.display());
    }
    if !skill_path.join("SKILL.md").exists() {
        bail!("SKILL.md not found in {}", skill_path.display());
    }

    println!("Validating skill...");
    validate_skill(&skill_path)?;
    println!("[OK] Skill is valid!\n");

    let skill_name = skill_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow::anyhow!("Could not determine skill name"))?;

    let out_dir = match output_dir {
        Some(d) => {
            fs::create_dir_all(d)?;
            PathBuf::from(d).canonicalize()?
        }
        None => std::env::current_dir()?,
    };
    let zip_path = out_dir.join(format!("{skill_name}.skill"));

    let zip_file =
        fs::File::create(&zip_path).with_context(|| format!("Creating {}", zip_path.display()))?;
    let mut zip = zip::ZipWriter::new(zip_file);
    let options = zip::write::FileOptions::<()>::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let parent = skill_path.parent().unwrap();
    for entry in WalkDir::new(&skill_path).min_depth(1).sort_by_file_name() {
        let entry = entry?;
        let file_path = entry.path();
        if file_path.is_file() {
            let arcname = file_path
                .strip_prefix(parent)?
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Non-UTF8 path"))?
                .replace('\\', "/");
            zip.start_file(&arcname, options)?;
            let data = fs::read(file_path)?;
            zip.write_all(&data)?;
            println!("  Added: {arcname}");
        }
    }
    zip.finish()?;
    println!(
        "\n[OK] Successfully packaged skill to: {}",
        zip_path.display()
    );
    Ok(())
}

// ─── GITHUB HELPERS ─────────────────────────────────────────────────────────

fn github_request(url: &str, user_agent: &str) -> Result<Vec<u8>> {
    let token = std::env::var("GITHUB_TOKEN")
        .or_else(|_| std::env::var("GH_TOKEN"))
        .ok();
    let mut req = ureq::get(url).set("User-Agent", user_agent);
    if let Some(tok) = &token {
        req = req.set("Authorization", &format!("token {tok}"));
    }
    let resp = req.call().with_context(|| format!("GET {url}"))?;
    let mut buf = Vec::new();
    resp.into_reader().read_to_end(&mut buf)?;
    Ok(buf)
}

// ─── LIST ───────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct GhContentsItem {
    name: String,
    #[serde(rename = "type")]
    kind: String,
}

fn codex_home() -> PathBuf {
    std::env::var("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".codex")
        })
}

fn installed_skills() -> std::collections::HashSet<String> {
    let root = codex_home().join("skills");
    if !root.is_dir() {
        return std::collections::HashSet::new();
    }
    fs::read_dir(&root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
        .collect()
}

fn cmd_list(repo: &str, skill_path: &str, ref_: &str, format: &str) -> Result<()> {
    let api_url = format!("https://api.github.com/repos/{repo}/contents/{skill_path}?ref={ref_}");
    let payload = github_request(&api_url, "codex-skill-list")
        .with_context(|| "Failed to fetch curated skills")?;
    let items: Vec<GhContentsItem> =
        serde_json::from_slice(&payload).with_context(|| "Unexpected curated listing response")?;
    let mut skills: Vec<String> = items
        .into_iter()
        .filter(|i| i.kind == "dir")
        .map(|i| i.name)
        .collect();
    skills.sort();

    let installed = installed_skills();
    match format {
        "json" => {
            let payload: Vec<serde_json::Value> = skills
                .iter()
                .map(
                    |name| serde_json::json!({"name": name, "installed": installed.contains(name)}),
                )
                .collect();
            println!("{}", serde_json::to_string(&payload)?);
        }
        _ => {
            for (i, name) in skills.iter().enumerate() {
                let suffix = if installed.contains(name) {
                    " (already installed)"
                } else {
                    ""
                };
                println!("{}. {}{}", i + 1, name, suffix);
            }
        }
    }
    Ok(())
}

// ─── INSTALL ────────────────────────────────────────────────────────────────

struct InstallSource {
    owner: String,
    repo: String,
    ref_: String,
    paths: Vec<String>,
}

fn parse_github_url(
    url: &str,
    default_ref: &str,
) -> Result<(String, String, String, Option<String>)> {
    let url = url::Url::parse(url).with_context(|| format!("Invalid URL: {url}"))?;
    if url.host_str() != Some("github.com") {
        bail!("Only GitHub URLs are supported.");
    }
    let parts: Vec<&str> = url.path().split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() < 2 {
        bail!("Invalid GitHub URL: need at least owner/repo.");
    }
    let (owner, repo) = (parts[0].to_string(), parts[1].to_string());
    let (ref_, subpath) = if parts.len() > 2 && (parts[2] == "tree" || parts[2] == "blob") {
        if parts.len() < 4 {
            bail!("GitHub URL missing ref or path.");
        }
        let r = parts[3].to_string();
        let sp = if parts.len() > 4 {
            Some(parts[4..].join("/"))
        } else {
            None
        };
        (r, sp)
    } else if parts.len() > 2 {
        (default_ref.to_string(), Some(parts[2..].join("/")))
    } else {
        (default_ref.to_string(), None)
    };
    Ok((owner, repo, ref_, subpath))
}

fn resolve_source(
    url: Option<&str>,
    repo: Option<&str>,
    paths: Option<&[String]>,
    ref_: &str,
) -> Result<InstallSource> {
    if let Some(u) = url {
        let (owner, repo_name, resolved_ref, url_path) = parse_github_url(u, ref_)?;
        let skill_paths = paths
            .map(|p| p.to_vec())
            .or_else(|| url_path.map(|p| vec![p]))
            .unwrap_or_default();
        if skill_paths.is_empty() {
            bail!("Missing --skill-path for GitHub URL.");
        }
        return Ok(InstallSource {
            owner,
            repo: repo_name,
            ref_: resolved_ref,
            paths: skill_paths,
        });
    }
    let repo = repo.ok_or_else(|| anyhow::anyhow!("Provide --repo or --url."))?;
    if repo.contains("://") {
        return resolve_source(Some(repo), None, paths, ref_);
    }
    let parts: Vec<&str> = repo.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() != 2 {
        bail!("--repo must be in owner/repo format.");
    }
    let skill_paths = paths
        .ok_or_else(|| anyhow::anyhow!("Missing --skill-path for --repo."))?
        .to_vec();
    Ok(InstallSource {
        owner: parts[0].to_string(),
        repo: parts[1].to_string(),
        ref_: ref_.to_string(),
        paths: skill_paths,
    })
}

fn download_repo_zip(owner: &str, repo: &str, ref_: &str, dest_dir: &Path) -> Result<PathBuf> {
    let zip_url = format!("https://codeload.github.com/{owner}/{repo}/zip/{ref_}");
    let payload = github_request(&zip_url, "codex-skill-install")?;
    let zip_path = dest_dir.join("repo.zip");
    fs::write(&zip_path, &payload)?;

    let mut top_levels = std::collections::HashSet::new();
    {
        let f = fs::File::open(&zip_path)?;
        let mut zip = zip::ZipArchive::new(f)?;
        let names: Vec<String> = (0..zip.len())
            .filter_map(|i| {
                zip.by_index(i)
                    .ok()
                    .and_then(|f| f.enclosed_name().map(|p| p.to_string_lossy().into_owned()))
            })
            .collect();
        for name in &names {
            if let Some(top) = name.split('/').next() {
                if !top.is_empty() {
                    top_levels.insert(top.to_string());
                }
            }
        }
        // safe extract
        let dest_real = dest_dir.canonicalize()?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            let out_path = dest_dir.join(entry.mangled_name());
            let out_real = out_path.canonicalize().unwrap_or(out_path.clone());
            if !out_real.starts_with(&dest_real) && !out_path.to_string_lossy().ends_with('/') {
                bail!("Archive contains unsafe path");
            }
            if entry.is_dir() {
                fs::create_dir_all(&out_path)?;
            } else {
                if let Some(p) = out_path.parent() {
                    fs::create_dir_all(p)?;
                }
                let mut f = fs::File::create(&out_path)?;
                std::io::copy(&mut entry, &mut f)?;
            }
        }
    }
    if top_levels.len() != 1 {
        bail!("Unexpected archive layout.");
    }
    Ok(dest_dir.join(top_levels.into_iter().next().unwrap()))
}

fn git_sparse_checkout(
    repo_url: &str,
    ref_: &str,
    paths: &[String],
    dest_dir: &Path,
) -> Result<PathBuf> {
    let repo_dir = dest_dir.join("repo");
    let status = std::process::Command::new("git")
        .args([
            "clone",
            "--filter=blob:none",
            "--depth",
            "1",
            "--sparse",
            "--single-branch",
            "--branch",
            ref_,
            repo_url,
        ])
        .arg(&repo_dir)
        .status();
    if !status.map(|s| s.success()).unwrap_or(false) {
        // try without --branch
        let s = std::process::Command::new("git")
            .args([
                "clone",
                "--filter=blob:none",
                "--depth",
                "1",
                "--sparse",
                "--single-branch",
                repo_url,
            ])
            .arg(&repo_dir)
            .status()?;
        if !s.success() {
            bail!("git clone failed");
        }
    }
    let s = std::process::Command::new("git")
        .args(["-C"])
        .arg(&repo_dir)
        .args(["sparse-checkout", "set"])
        .args(paths)
        .status()?;
    if !s.success() {
        bail!("git sparse-checkout failed");
    }
    let s = std::process::Command::new("git")
        .args(["-C"])
        .arg(&repo_dir)
        .args(["checkout", ref_])
        .status()?;
    if !s.success() {
        bail!("git checkout failed");
    }
    Ok(repo_dir)
}

fn prepare_repo(source: &InstallSource, method: &str, tmp_dir: &Path) -> Result<PathBuf> {
    if method == "download" || method == "auto" {
        match download_repo_zip(&source.owner, &source.repo, &source.ref_, tmp_dir) {
            Ok(p) => return Ok(p),
            Err(e) => {
                if method == "download" {
                    return Err(e);
                }
                eprintln!("download failed ({e}), trying git...");
            }
        }
    }
    if method == "git" || method == "auto" {
        let repo_url = format!("https://github.com/{}/{}.git", source.owner, source.repo);
        match git_sparse_checkout(&repo_url, &source.ref_, &source.paths, tmp_dir) {
            Ok(p) => return Ok(p),
            Err(_) => {
                let ssh_url = format!("git@github.com:{}/{}.git", source.owner, source.repo);
                return git_sparse_checkout(&ssh_url, &source.ref_, &source.paths, tmp_dir);
            }
        }
    }
    bail!("Unsupported method: {method}");
}

fn cmd_install(
    url: Option<&str>,
    repo: Option<&str>,
    skill_path: Option<&[String]>,
    ref_: &str,
    dest: Option<&str>,
    name: Option<&str>,
    method: &str,
) -> Result<()> {
    let source = resolve_source(url, repo, skill_path, ref_)?;
    if source.paths.is_empty() {
        bail!("No skill paths provided.");
    }

    for p in &source.paths {
        if Path::new(p).is_absolute() || p.starts_with("..") {
            bail!("Skill path must be relative inside the repo.");
        }
    }

    let dest_root = dest
        .map(PathBuf::from)
        .unwrap_or_else(|| codex_home().join("skills"));

    let tmp = tempfile::tempdir()?;
    let repo_root = prepare_repo(&source, method, tmp.path())?;

    let mut installed = vec![];
    for skill_p in &source.paths {
        let skill_name = if source.paths.len() == 1 { name } else { None }
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                Path::new(skill_p)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(skill_p)
                    .trim_end_matches('/')
                    .to_string()
            });
        if skill_name.is_empty() || skill_name == "." || skill_name == ".." {
            bail!("Invalid skill name: '{skill_name}'");
        }
        let src = repo_root.join(skill_p);
        if !src.is_dir() {
            bail!("Skill path not found: {}", src.display());
        }
        if !src.join("SKILL.md").is_file() {
            bail!("SKILL.md not found in {}", src.display());
        }
        let dest_dir = dest_root.join(&skill_name);
        if dest_dir.exists() {
            bail!("Destination already exists: {}", dest_dir.display());
        }
        fs::create_dir_all(dest_root.as_path())?;
        copy_dir(&src, &dest_dir)?;
        installed.push((skill_name, dest_dir));
    }

    for (name, dir) in &installed {
        println!("[OK] Installed {} to {}", name, dir.display());
    }
    Ok(())
}

fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in WalkDir::new(src).min_depth(1) {
        let entry = entry?;
        let rel = entry.path().strip_prefix(src)?;
        let target = dst.join(rel);
        if entry.path().is_dir() {
            fs::create_dir_all(&target)?;
        } else {
            if let Some(p) = target.parent() {
                fs::create_dir_all(p)?;
            }
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}
