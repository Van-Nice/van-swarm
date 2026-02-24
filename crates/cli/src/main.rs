//! VanSwarm CLI — scaffold agent projects.
//!
//! Run from workspace: `cargo run -p vanswarm-cli -- new my_agent`
//! Or install: `cargo install --path crates/cli` then `vanswarm new my_agent`

mod cursor_rules;

use std::fs;
use std::path::Path;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "vanswarm")]
#[command(about = "VanSwarm framework CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize current (or given) directory as a VanSwarm project: MCP config, .cursor/rules (Rust), and optional libsql data dir.
    Init {
        /// Directory to initialize (default: current directory).
        #[arg(default_value = ".")]
        path: String,

        /// Only add MCP config and .vanswarm/data; skip creating vanswarm.toml if directory already has one.
        #[arg(long)]
        mcp_only: bool,

        /// Overwrite existing .cursor/mcp.json or vanswarm.toml.
        #[arg(long)]
        overwrite: bool,

        /// Do not set up .vanswarm/data or VANSWARM_DB_PATH (in-memory only).
        #[arg(long)]
        no_libsql: bool,

        /// Do not add .cursor/rules (Rust basics, cargo workflow, refactor, architecture, rust-mcp routing).
        #[arg(long)]
        no_cursor_rules: bool,

        /// Log each file created or updated.
        #[arg(short, long)]
        verbose: bool,
    },

    /// Create a new agent project with boilerplate.
    New {
        /// Project name (folder and crate name; use underscores for crate).
        name: String,

        /// Output directory (default: ./<name>).
        #[arg(short, long)]
        path: Option<String>,

        /// LLM provider: anthropic, openai, gemini.
        #[arg(long, default_value = "anthropic")]
        provider: Provider,

        /// Model id (default: provider default).
        #[arg(long)]
        model: Option<String>,

        /// Add a sample custom tool in src/tools.rs.
        #[arg(long)]
        with_tools: bool,

        /// Add vanswarm-mcp dependency and MCP example.
        #[arg(long)]
        with_mcp: bool,

        /// Add vanswarm-memory dependency and EpisodicMemory snippet.
        #[arg(long)]
        with_memory: bool,

        /// Generate a library crate with examples/run_agent.rs.
        #[arg(long)]
        lib: bool,

        /// Framework dependency: path (relative) or git.
        #[arg(long, default_value = "path")]
        framework_path: FrameworkPath,

        /// Skip generating README.md.
        #[arg(long)]
        no_readme: bool,

        /// Skip generating .env.example.
        #[arg(long)]
        no_env_example: bool,

        /// Overwrite existing directory.
        #[arg(short, long)]
        force: bool,

        /// Log each file created.
        #[arg(short, long)]
        verbose: bool,
    },
}

#[derive(Clone, Default)]
enum Provider {
    #[default]
    Anthropic,
    OpenAI,
    Gemini,
}

impl std::str::FromStr for Provider {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "anthropic" => Ok(Provider::Anthropic),
            "openai" => Ok(Provider::OpenAI),
            "gemini" => Ok(Provider::Gemini),
            _ => Err(format!("unknown provider: {}", s)),
        }
    }
}

impl Provider {
    fn default_model(&self) -> &'static str {
        match self {
            Provider::Anthropic => "claude-sonnet-4-20250514",
            Provider::OpenAI => "gpt-4o",
            Provider::Gemini => "gemini-2.0-flash",
        }
    }
}

#[derive(Clone, Default)]
enum FrameworkPath {
    #[default]
    Path,
    Git,
}

impl std::str::FromStr for FrameworkPath {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "path" => Ok(FrameworkPath::Path),
            "git" => Ok(FrameworkPath::Git),
            _ => Err(format!("expected path or git: {}", s)),
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Init {
            path,
            mcp_only,
            overwrite,
            no_libsql,
            no_cursor_rules,
            verbose,
        } => run_init(path, mcp_only, overwrite, no_libsql, no_cursor_rules, verbose)?,
        Commands::New {
            name,
            path,
            provider,
            model,
            with_tools,
            with_mcp,
            with_memory,
            lib,
            framework_path,
            no_readme,
            no_env_example,
            force,
            verbose,
        } => run_new(
            name,
            path,
            provider,
            model,
            with_tools,
            with_mcp,
            with_memory,
            lib,
            framework_path,
            no_readme,
            no_env_example,
            force,
            verbose,
        )?,
    }
    Ok(())
}

fn run_new(
    name: String,
    path: Option<String>,
    provider: Provider,
    model: Option<String>,
    with_tools: bool,
    with_mcp: bool,
    with_memory: bool,
    lib: bool,
    framework_path: FrameworkPath,
    no_readme: bool,
    no_env_example: bool,
    force: bool,
    verbose: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let crate_name = name.replace('-', "_");
    let out_dir = path.unwrap_or_else(|| name.clone());
    let root = Path::new(&out_dir);

    if root.exists() && !force {
        return Err(format!("directory already exists: {} (use --force to overwrite)", out_dir).into());
    }

    let model_id = model.unwrap_or_else(|| provider.default_model().to_string());

    let log = |msg: &str| {
        if verbose {
            println!("  {}", msg);
        }
    };

    println!("Creating agent project '{}' in {}", crate_name, out_dir);

    fs::create_dir_all(root.join("src"))?;
    log("src/");

    // Cargo.toml
    let cargo = cargo_toml(&crate_name, &framework_path, with_mcp, with_memory, with_tools);
    fs::write(root.join("Cargo.toml"), cargo)?;
    log("Cargo.toml");

    // main.rs or lib.rs
    let main_rs = main_rs(&provider, &model_id, with_tools, with_mcp, with_memory, lib);
    if lib {
        fs::write(root.join("src/lib.rs"), lib_rs(&provider, &model_id, with_tools))?;
        log("src/lib.rs");
        fs::create_dir_all(root.join("examples"))?;
        fs::write(root.join("examples/run_agent.rs"), main_rs)?;
        log("examples/run_agent.rs");
    } else {
        fs::write(root.join("src/main.rs"), main_rs)?;
        log("src/main.rs");
    }

    if with_tools {
        let tools_rs = tools_rs();
        fs::write(root.join("src/tools.rs"), tools_rs)?;
        log("src/tools.rs");
    }

    if !no_env_example {
        fs::write(root.join(".env.example"), env_example())?;
        log(".env.example");
    }

    if !no_readme {
        fs::write(
            root.join("README.md"),
            readme(&crate_name, &provider, with_tools, with_mcp, with_memory, lib),
        )?;
        log("README.md");
    }

    println!("Done. Next: cd {} && cp .env.example .env && set your API key, then cargo run", out_dir);
    Ok(())
}

fn run_init(
    path: String,
    mcp_only: bool,
    overwrite: bool,
    no_libsql: bool,
    no_cursor_rules: bool,
    verbose: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(&path);
    if !root.exists() {
        fs::create_dir_all(root)?;
    }
    if !root.is_dir() {
        return Err(format!("{} is not a directory", path).into());
    }

    let log = |msg: &str| {
        if verbose {
            println!("  {}", msg);
        }
    };

    println!("Initializing VanSwarm project in {}", root.display());

    // .vanswarm/data/
    if !no_libsql {
        let data_dir = root.join(".vanswarm").join("data");
        fs::create_dir_all(&data_dir)?;
        log(".vanswarm/data/");
        let gitkeep = data_dir.join(".gitkeep");
        if !gitkeep.exists() {
            fs::write(gitkeep, "")?;
            log(".vanswarm/data/.gitkeep");
        }
        // .gitignore
        let gitignore_path = root.join(".gitignore");
        let db_ignore = ".vanswarm/data/*.db";
        if gitignore_path.exists() {
            let content = fs::read_to_string(&gitignore_path)?;
            if !content.contains(db_ignore) {
                let appended = format!("{}\n{}\n", content.trim_end(), db_ignore);
                fs::write(&gitignore_path, appended)?;
                log(".gitignore (appended)");
            }
        } else {
            fs::write(&gitignore_path, format!("{}\n", db_ignore))?;
            log(".gitignore");
        }
    }

    // .cursor/rules (same as rust-mcp cursor_init_rules)
    if !no_cursor_rules {
        let rules_dir = root.join(".cursor").join("rules");
        fs::create_dir_all(&rules_dir)?;
        log(".cursor/rules/");
        for (name, content) in cursor_rules::all_rules() {
            let p = rules_dir.join(name);
            if overwrite || !p.exists() {
                fs::write(&p, content)?;
                log(&format!(".cursor/rules/{}", name));
            }
        }
    }

    // .cursor/mcp.json
    let cursor_dir = root.join(".cursor");
    fs::create_dir_all(&cursor_dir)?;
    let mcp_json_path = cursor_dir.join("mcp.json");
    let db_path = if no_libsql {
        None
    } else {
        Some(".vanswarm/data/vanswarm.db".to_string())
    };
    let mcp_json = mcp_json_content(db_path.as_deref());
    if overwrite || !mcp_json_path.exists() {
        fs::write(&mcp_json_path, mcp_json)?;
        log(".cursor/mcp.json");
    }

    // vanswarm.toml
    if !mcp_only || !root.join("vanswarm.toml").exists() {
        let toml_path = root.join("vanswarm.toml");
        if overwrite || !toml_path.exists() {
            let toml_content = vanswarm_toml_content(no_libsql);
            fs::write(&toml_path, toml_content)?;
            log("vanswarm.toml");
        }
    }

    println!("Done. MCP: .cursor/mcp.json. Rules: .cursor/rules/. Set VANSWARM_DB_PATH in env or mcp.json for persistent memory.");
    Ok(())
}

fn mcp_json_content(db_path: Option<&str>) -> String {
    let env = if let Some(p) = db_path {
        format!(r#", "env": {{ "VANSWARM_DB_PATH": "{}" }}"#, p)
    } else {
        String::new()
    };
    // format!: "{{" → "{", "}}" → "}"
    let s = format!(
        r#"{{{{
  "mcpServers": {{{{
    "vanswarm": {{{{
      "command": "vanswarm-mcp-server"{}
    }}}}
  }}}}
}}"#,
        env
    );
    s.replace("{{", "{").replace("}}", "}")
}

fn vanswarm_toml_content(no_libsql: bool) -> String {
    let mcp_section = if no_libsql {
        r#"
# [mcp]
# server_path = "vanswarm-mcp-server"
"#.to_string()
    } else {
        r#"
[mcp]
# server_path = "vanswarm-mcp-server"
db_path = ".vanswarm/data/vanswarm.db"
"#.to_string()
    };
    format!(
        r#"[project]
name = "vanswarm-project"
{}
"#,
        mcp_section
    )
}

fn cargo_toml(
    crate_name: &str,
    framework_path: &FrameworkPath,
    with_mcp: bool,
    with_memory: bool,
    with_tools: bool,
) -> String {
    let core_dep = match framework_path {
        FrameworkPath::Path => r#"vanswarm-core = { path = "../crates/core" }"#.to_string(),
        FrameworkPath::Git => r#"vanswarm-core = { git = "https://github.com/Van-Nice/van-swarm", branch = "master" }"#.to_string(),
    };
    let mut deps = format!(
        r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[workspace]

[dependencies]
{}
tokio = {{ version = "1", features = ["full"] }}
"#,
        crate_name, core_dep
    );
    if with_mcp {
        let mcp = match framework_path {
            FrameworkPath::Path => r#"vanswarm-mcp = { path = "../crates/mcp" }"#,
            FrameworkPath::Git => r#"vanswarm-mcp = { git = "https://github.com/Van-Nice/van-swarm", branch = "master" }"#,
        };
        deps.push_str("\n");
        deps.push_str(mcp);
        deps.push_str("\n");
    }
    if with_memory {
        let mem = match framework_path {
            FrameworkPath::Path => r#"vanswarm-memory = { path = "../crates/memory" }"#,
            FrameworkPath::Git => r#"vanswarm-memory = { git = "https://github.com/Van-Nice/van-swarm", branch = "master" }"#,
        };
        deps.push_str("\n");
        deps.push_str(mem);
        deps.push_str("\n");
    }
    if with_tools {
        deps.push_str("\nasync-trait = \"0.1\"\n");
        deps.push_str("serde_json = \"1\"\n");
    }
    deps
}

fn main_rs(
    provider: &Provider,
    model_id: &str,
    with_tools: bool,
    _with_mcp: bool,
    _with_memory: bool,
    is_example: bool,
) -> String {
    let (provider_type, provider_ctor) = match provider {
        Provider::Anthropic => ("AnthropicProvider", "AnthropicProvider::from_env()?"),
        Provider::OpenAI => ("OpenAiProvider", "OpenAiProvider::from_env()?"),
        Provider::Gemini => ("GeminiProvider", "GeminiProvider::from_env()?"),
    };
    let executor = if with_tools {
        "LocalToolRegistry::new().register(tools::GreetTool)"
    } else {
        "LocalToolRegistry::new()"
    };
    let tools_mod = if with_tools { "\nmod tools;\n" } else { "" };
    let run_prompt = if is_example {
        r#"    let answer = run_agent(&agent, "Hello! Greet the user and say the current task.").await?;"#
    } else {
        r#"    let answer = run_agent(&agent, "Hello! Reply in one short sentence.").await?;"#
    };
    format!(
        r#"//! Generated by vanswarm new. Run: cargo run
//! Set one of: ANTHROPIC_API_KEY, OPENAI_API_KEY, GEMINI_API_KEY.
{}

use std::sync::Arc;

use vanswarm_core::{{
    config::{{AgentConfig, ModelConfig}},
    providers::{},
    react::{{run_agent, ReActAgent}},
    traits::tool::LocalToolRegistry,
}};

#[tokio::main]
async fn main() -> vanswarm_core::Result<()> {{
    let provider = {};
    let executor = {};
    let config = AgentConfig::new("agent", ModelConfig::new("{}"))
        .with_max_iterations(10);
    let agent = ReActAgent::new(config, Arc::new(provider), Arc::new(executor));

    {}
    println!("{{}}", answer);
    Ok(())
}}
"#,
        tools_mod,
        provider_type,
        provider_ctor,
        executor,
        model_id,
        run_prompt
    )
}

fn lib_rs(provider: &Provider, model_id: &str, with_tools: bool) -> String {
    let (provider_type, provider_ctor) = match provider {
        Provider::Anthropic => ("AnthropicProvider", "AnthropicProvider::from_env()?"),
        Provider::OpenAI => ("OpenAiProvider", "OpenAiProvider::from_env()?"),
        Provider::Gemini => ("GeminiProvider", "GeminiProvider::from_env()?"),
    };
    let executor = if with_tools {
        "LocalToolRegistry::new().register(tools::GreetTool)"
    } else {
        "LocalToolRegistry::new()"
    };
    let tools_mod = if with_tools { "\npub mod tools;\n" } else { "" };
    format!(
        r#"//! Agent library — build the agent and run it from examples/run_agent.rs.
{}

use std::sync::Arc;

use vanswarm_core::{{
    config::{{AgentConfig, ModelConfig}},
    providers::{},
    react::{{run_agent, ReActAgent}},
    traits::tool::LocalToolRegistry,
}};

/// Build and run the agent with the given prompt.
pub async fn run(prompt: &str) -> vanswarm_core::Result<String> {{
    let provider = {};
    let executor = {};
    let config = AgentConfig::new("agent", ModelConfig::new("{}"))
        .with_max_iterations(10);
    let agent = ReActAgent::new(config, Arc::new(provider), Arc::new(executor));
    run_agent(&agent, prompt).await
}}
"#,
        tools_mod,
        provider_type,
        provider_ctor,
        executor,
        model_id
    )
}

fn tools_rs() -> String {
    r#"//! Sample custom tool — implement more in this module.

use async_trait::async_trait;
use vanswarm_core::{message::ToolDefinition, traits::tool::Tool};

/// A simple greeting tool.
pub struct GreetTool;

#[async_trait]
impl Tool for GreetTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "greet".into(),
            description: "Greet someone by name.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": { "name": { "type": "string", "description": "Name to greet" } },
                "required": ["name"]
            }),
            examples: vec![],
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> vanswarm_core::Result<String> {
        let name = arguments.get("name").and_then(|v| v.as_str()).unwrap_or("World");
        Ok(format!("Hello, {}!", name))
    }
}
"#.to_string()
}

fn env_example() -> String {
    r#"# Set one of these (depending on your --provider):
# ANTHROPIC_API_KEY=sk-ant-...
# OPENAI_API_KEY=sk-...
# GEMINI_API_KEY=...
"#.to_string()
}

fn readme(
    crate_name: &str,
    provider: &Provider,
    with_tools: bool,
    _with_mcp: bool,
    _with_memory: bool,
    lib: bool,
) -> String {
    let env_var = match provider {
        Provider::Anthropic => "ANTHROPIC_API_KEY",
        Provider::OpenAI => "OPENAI_API_KEY",
        Provider::Gemini => "GEMINI_API_KEY",
    };
    let run_cmd = if lib {
        "cargo run --example run_agent"
    } else {
        "cargo run"
    };
    let mut s = format!(
        r#"# {}

Agent project generated by [vanswarm new](https://github.com/Van-Nice/van-swarm).

## Run

1. Copy env and set your API key:
   ```sh
   cp .env.example .env
   export {}="your-key"
   ```
2. Run the agent:
   ```sh
   {}
   ```
"#,
        crate_name.replace('_', " "),
        env_var,
        run_cmd
    );
    if with_tools {
        s.push_str(
            r#"
## Custom tools

Edit `src/tools.rs` to add or change tools, then register them in the executor in `src/main.rs` (or `src/lib.rs`).
"#,
        );
    }
    s
}
