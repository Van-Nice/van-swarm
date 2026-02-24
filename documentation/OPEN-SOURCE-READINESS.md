# Open source readiness — Making the repo public on GitHub

This checklist helps you confirm the codebase is ready to be made **public** on GitHub.

---

## Already in place

| Item | Status |
|------|--------|
| **License** | Declared as MIT OR Apache-2.0 in root `Cargo.toml` and README. Add `LICENSE-MIT` and `LICENSE-APACHE` files (see below). |
| **README** | Clear project name (VanSwarm), crate map, quick start, architecture, link to docs. |
| **CONTRIBUTING.md** | Development setup, how to submit, code of conduct reference. |
| **CODE_OF_CONDUCT.md** | Contributor Covenant. |
| **No committed secrets** | Credentials are env-based (e.g. `ANTHROPIC_API_KEY`); no API keys in repo. `.env` is gitignored; examples use placeholders. |
| **.gitignore** | Targets, env files, IDE, logs, and scaffolded `test_agent/` ignored. |
| **Documentation** | `documentation/` with guides, architecture, proposals, HOW-IT-WORKS, PLATFORM-FEATURES, CODEBASE-READING-ORDER. |
| **CI** | Checklist §1.6 references CI (test, fmt, clippy, doc); ensure your GitHub repo has a workflow that runs these. |

---

## Do before publishing

### 1. Add license files

The workspace declares `license = "MIT OR Apache-2.0"`. Add the actual license texts at the repo root:

- **LICENSE-MIT** — MIT license text with year and copyright holder (e.g. `Copyright (c) 2025 VanSwarm Contributors`).
- **LICENSE-APACHE** — Full Apache-2.0 text (and NOTICE if you use it). Optional: add a short NOTICE file if the project incorporates Apache-2.0–licensed third-party code that requires attribution.

Many Rust projects use the standard texts from https://choosealicense.com or copy from the Rust project.

### 2. Set the repository URL

In root **Cargo.toml** you have:

```toml
repository = "https://github.com/your-org/rust-agent-framework"
```

Before or right after creating the **public** GitHub repo:

- Replace `your-org` with your GitHub org or username.
- Replace `rust-agent-framework` with the actual repo name if different (e.g. `vanswarm`, `rust-agent-framework`).

Example:

```toml
repository = "https://github.com/yourusername/vanswarm"
```

After the first push, `cargo doc` and crates.io (if you publish) will link to the correct repo.

### 3. Optional: refresh README status and checklist

- **Status line:** README says "Active development — Phase 1 (foundations) in progress." You can soften this for public launch, e.g. "Early adopters welcome — core features ready; see documentation/guides/ for what's implemented."
- **Checklist:** The README checklist has many unchecked items even though the build checklist shows many done. Consider syncing or pointing to `documentation/FRAMEWORK-BUILD-CHECKLIST.md` as the source of truth so new contributors aren’t confused.

### 4. Confirm no sensitive paths are tracked

- Run `git status` and `git log --all --full-history -- .env` (and any path with "secret", "key", "credential") to ensure no env or key files were ever committed.
- If the repo was ever private with internal URLs or keys, run a quick search (e.g. `grep -r "sk-ant-" --include="*.rs" --include="*.toml" .` or use GitHub’s secret scanning) and rewrite history if needed (e.g. BFG or git filter-repo).

### 5. GitHub repo settings

- Create the repo (e.g. `vanswarm` or `rust-agent-framework`) under your org/user.
- Set visibility to **Public**.
- Add a short description and topics (e.g. `rust`, `ai`, `agents`, `llm`, `mcp`).
- Optionally add a **GitHub Actions** workflow for `cargo test`, `cargo fmt -- --check`, and `cargo clippy` so the README/CONTRIBUTING “CI passes” claim is true.

---

## After going public

- **Publishing to crates.io:** When you’re ready to publish the crates (e.g. `vanswarm-core`, `vanswarm-mcp`), ensure each crate’s `Cargo.toml` has a non-placeholder `repository` and that the license and description are correct. You can start with a single umbrella crate or publish core first.
- **Security:** Enable Dependabot and optionally code scanning if you want extra assurance.
- **Community:** Pin CONTRIBUTING.md and CODE_OF_CONDUCT.md in the repo so new contributors see them.

---

## Summary

| Action | Required? |
|--------|-----------|
| Add LICENSE-MIT and LICENSE-APACHE | **Yes** |
| Set `repository` in root Cargo.toml to real GitHub URL | **Yes** |
| Confirm no secrets in repo / history | **Yes** |
| Tweak README status/checklist | Optional |
| Add or verify CI workflow | Recommended |
| Publish to crates.io | When you’re ready |

Once the license files and repository URL are in place and you’ve confirmed there are no secrets, the project is in good shape to be made **public on GitHub**.
