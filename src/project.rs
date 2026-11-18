//! Descobre o projeto por trás de um processo: nome, framework, linguagem e branch do git.
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct Project {
    pub root: PathBuf,
    pub name: String,
    pub stack: Option<String>,
    pub lang: &'static str,
    pub branch: Option<String>,
}

#[derive(Default)]
pub struct ProjectCache {
    map: HashMap<PathBuf, (Instant, Option<Project>)>,
}

impl ProjectCache {
    pub fn get(&mut self, cwd: &Path) -> Option<Project> {
        if let Some((t, p)) = self.map.get(cwd) {
            if t.elapsed() < Duration::from_secs(15) {
                return p.clone();
            }
        }
        let p = detect(cwd);
        self.map.insert(cwd.to_path_buf(), (Instant::now(), p.clone()));
        p
    }
}

pub fn detect(cwd: &Path) -> Option<Project> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut dir = Some(cwd);
    for _ in 0..6 {
        let d = dir?;
        if Some(d) == home.as_deref() || d == Path::new("/") {
            break;
        }
        if let Some(p) = inspect(d) {
            return Some(p);
        }
        dir = d.parent();
    }
    None
}

fn find_in(text: &str, table: &[(&str, &str)]) -> Option<String> {
    let low = text.to_lowercase();
    table.iter().find(|(k, _)| low.contains(k)).map(|(_, n)| n.to_string())
}

fn inspect(d: &Path) -> Option<Project> {
    let dirname = d.file_name()?.to_string_lossy().to_string();
    let read = |f: &str| fs::read_to_string(d.join(f)).ok();

    let (name, stack, lang): (String, Option<String>, &'static str) = if let Some(s) = read("package.json") {
        let v: serde_json::Value = serde_json::from_str(&s).unwrap_or_default();
        let has = |k: &str| v["dependencies"].get(k).is_some() || v["devDependencies"].get(k).is_some();
        let stack = [
            ("next", "Next.js"),
            ("nuxt", "Nuxt"),
            ("@remix-run/react", "Remix"),
            ("@sveltejs/kit", "SvelteKit"),
            ("astro", "Astro"),
            ("@docusaurus/core", "Docusaurus"),
            ("vitepress", "VitePress"),
            ("@nestjs/core", "NestJS"),
            ("hono", "Hono"),
            ("fastify", "Fastify"),
            ("express", "Express"),
            ("@angular/core", "Angular"),
            ("@modelcontextprotocol/sdk", "MCP server"),
            ("electron", "Electron"),
            ("vite", "Vite"),
            ("react-scripts", "Create React App"),
        ]
        .iter()
        .find(|(k, _)| has(k))
        .map(|(_, n)| n.to_string());
        let lang = if d.join("bun.lockb").exists() || d.join("bun.lock").exists() {
            "Bun"
        } else if has("typescript") {
            "TypeScript"
        } else {
            "JavaScript"
        };
        (v["name"].as_str().unwrap_or(&dirname).to_string(), stack, lang)
    } else if let Some(s) = read("Cargo.toml") {
        let name = s
            .lines()
            .find(|l| l.trim_start().starts_with("name"))
            .and_then(|l| l.split('"').nth(1))
            .unwrap_or(&dirname)
            .to_string();
        let stack = find_in(
            &s,
            &[("axum", "Axum"), ("actix-web", "Actix"), ("rocket", "Rocket"), ("leptos", "Leptos"), ("tauri", "Tauri")],
        );
        (name, stack, "Rust")
    } else if let Some(s) = read("go.mod") {
        let name = s
            .lines()
            .next()
            .and_then(|l| l.strip_prefix("module "))
            .and_then(|m| m.rsplit('/').next())
            .unwrap_or(&dirname)
            .to_string();
        let stack =
            find_in(&s, &[("gin-gonic", "Gin"), ("labstack/echo", "Echo"), ("gofiber", "Fiber"), ("go-chi", "Chi")]);
        (name, stack, "Go")
    } else if let Some(s) = read("mix.exs") {
        (dirname, s.contains(":phoenix").then(|| "Phoenix".to_string()), "Elixir")
    } else if let Some(s) = read("pyproject.toml").or_else(|| read("requirements.txt")) {
        let stack = find_in(
            &s,
            &[
                ("fastapi", "FastAPI"),
                ("django", "Django"),
                ("flask", "Flask"),
                ("streamlit", "Streamlit"),
                ("langchain", "LangChain"),
                ("uvicorn", "ASGI"),
            ],
        );
        (dirname, stack, "Python")
    } else if read("pubspec.yaml").is_some() {
        (dirname, None, "Flutter")
    } else {
        let s = read("composer.json")?;
        (dirname, s.contains("laravel/framework").then(|| "Laravel".to_string()), "PHP")
    };

    Some(Project { root: d.to_path_buf(), name, stack, lang, branch: git_branch(d) })
}

fn git_branch(d: &Path) -> Option<String> {
    let mut cur = Some(d);
    while let Some(c) = cur {
        let g = c.join(".git");
        let gitdir = if g.is_dir() {
            Some(g)
        } else if g.is_file() {
            // worktree/submódulo: arquivo ".git" com "gitdir: caminho"
            let t = fs::read_to_string(&g).ok()?;
            let p = PathBuf::from(t.trim().strip_prefix("gitdir: ")?);
            Some(if p.is_absolute() { p } else { c.join(p) })
        } else {
            None
        };
        if let Some(gd) = gitdir {
            let head = fs::read_to_string(gd.join("HEAD")).ok()?;
            let head = head.trim();
            return Some(match head.strip_prefix("ref: refs/heads/") {
                Some(b) => b.to_string(),
                None => format!("@{}", &head[..head.len().min(7)]),
            });
        }
        cur = c.parent();
    }
    None
}
