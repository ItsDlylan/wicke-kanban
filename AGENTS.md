# Repository Guidelines

## Project Structure & Module Organization
- `crates/`: Rust workspace crates — `server` (API + bins), `db` (SQLx models/migrations), `executors`, `services`, `utils`, `git` (Git operations), `api-types` (shared API types for local + remote), `review` (PR review tool), `deployment`, `local-deployment`, `remote`.
- `frontend/`: React + TypeScript app (Vite, Tailwind). Source in `frontend/src`.
- `frontend/src/components/dialogs`: Dialog components for the frontend.
- `remote-frontend/`: Remote deployment frontend.
- `shared/`: Generated TypeScript types (`shared/types.ts`, `shared/remote-types.ts`) and agent tool schemas (`shared/schemas/`). Do not edit generated files directly.
- `assets/`, `dev_assets_seed/`, `dev_assets/`: Packaged and local dev assets.
- `npx-cli/`: Files published to the npm CLI package.
- `scripts/`: Dev helpers (ports, DB preparation).
- `docs/`: Documentation files.

### Crate-specific guides
- [`crates/remote/AGENTS.md`](crates/remote/AGENTS.md) — Remote server architecture, ElectricSQL integration, mutation patterns, environment variables.
- [`docs/AGENTS.md`](docs/AGENTS.md) — Mintlify documentation writing guidelines and component reference.
- [`frontend/AGENTS.md`](frontend/AGENTS.md) — Frontend design system styling guidelines.

## Managing Shared Types Between Rust and TypeScript

ts-rs allows you to derive TypeScript types from Rust structs/enums. By annotating your Rust types with #[derive(TS)] and related macros, ts-rs will generate .ts declaration files for those types.
When making changes to the types, you can regenerate them using `pnpm run generate-types`
Do not manually edit shared/types.ts, instead edit crates/server/src/bin/generate_types.rs

For remote/cloud types, regenerate using `pnpm run remote:generate-types`
Do not manually edit shared/remote-types.ts, instead edit crates/remote/src/bin/remote-generate-types.rs (see crates/remote/AGENTS.md for details).

## Build, Test, and Development Commands
- Install: `pnpm i`
- Run dev (frontend + backend with ports auto-assigned): `pnpm run dev`
- Backend (watch): `pnpm run backend:dev:watch`
- Frontend (dev): `pnpm run frontend:dev`
- Type checks: `pnpm run check` (frontend) and `pnpm run backend:check` (Rust cargo check)
- Rust tests: `cargo test --workspace`
- Generate TS types from Rust: `pnpm run generate-types` (or `generate-types:check` in CI)
- Prepare SQLx (offline): `pnpm run prepare-db`
- Prepare SQLx (remote package, postgres): `pnpm run remote:prepare-db`
- Local NPX build: `pnpm run build:npx` then `pnpm pack` in `npx-cli/`
- Format code: `pnpm run format` (runs `cargo fmt` + frontend Prettier)
- Lint: `pnpm run lint` (runs frontend ESLint + `cargo clippy`)

## Before Completing a Task
- Run `pnpm run format` to format all Rust and frontend code.
- Run `pnpm run backend:check` to ensure Rust changes compile.

## Worktree Workflow (Isolation Tiers)

**Rule: When you are a teammate agent assigned a task, you MUST use `/tier2` to create an isolated worktree before writing any code.** Only skip this for read-only tasks (code review, documentation, research). Default to Tier 2 for any task that modifies code — feature work, bug fixes, refactors, migrations.

Use git worktrees to isolate development environments. Each worktree gets its own branch, database, ports, and dependencies.

### Teammate Workflow
1. Receive task assignment
2. Run `/tier2` to create an isolated worktree (you'll be asked for a branch name)
3. `cd` into the worktree directory
4. Do the work
5. Commit and push from the worktree
6. Report back to team lead

### Tier 1 — Shared Environment
- **Use for:** code review, docs, reading, research — **read-only tasks only**
- **Database:** shared `dev_assets/db.sqlite`
- **URL:** `http://localhost:<port>` (from main `.dev-ports.json`)
- **Setup:** none needed — work directly in the main checkout

### Tier 2 — Isolated Worktree + Database (Default for Code Changes)
- **Use for:** feature development, bug fixes, refactors, database migrations, testing with data
- **Database:** own `dev_assets/db.sqlite` (copied from `dev_assets_seed/`)
- **URL:** `http://localhost:<unique-port>` (auto-allocated)
- **Creation:** run `/tier2` or `bin/worktree create feature/my-feature`

### Tier 3 — Worktree with Shared Database
- **Use for:** frontend-only changes, CSS/Tailwind, React components
- **Database:** symlinked to main `dev_assets/`
- **URL:** `http://localhost:<unique-port>` (auto-allocated)
- **Creation:** `bin/worktree create feature/ui-update --no-db`

### Decision Checklist
1. Does the task modify any code? → **Tier 2** (default)
2. Is it frontend-only with no data concerns? → **Tier 3**
3. Is it a quick read/review/doc change? → **Tier 1**

### Worktree Management Commands
- `bin/worktree create <branch> [--from <base>] [--no-db]` — Create worktree
- `bin/worktree list` — List all worktrees with status
- `bin/worktree info <branch>` — Show worktree details
- `bin/worktree url [<branch>]` — Output dev URL
- `bin/worktree remove <branch>` — Remove worktree + cleanup
- `bin/worktree cleanup [--dry-run]` — Remove merged worktrees
- `bin/install-hooks` — Install git hooks (post-merge auto-cleanup)

## Coding Style & Naming Conventions
- Rust: `rustfmt` enforced (`rustfmt.toml`); group imports by crate; snake_case modules, PascalCase types.
- TypeScript/React: ESLint + Prettier (2 spaces, single quotes, 80 cols). PascalCase components, camelCase vars/functions, kebab-case file names where practical.
- Keep functions small, add `Debug`/`Serialize`/`Deserialize` where useful.

## Testing Guidelines
- Rust: prefer unit tests alongside code (`#[cfg(test)]`), run `cargo test --workspace`. Add tests for new logic and edge cases.
- Frontend: ensure `pnpm run check` and `pnpm run lint` pass. If adding runtime logic, include lightweight tests (e.g., Vitest) in the same directory.

## Git Commits
- Do NOT add "Co-Authored-By" lines or any AI attribution to commit messages
- Do NOT add "Generated with Claude Code" or similar AI footers
- Keep commit messages clean and professional

## Browser Automation (agent-browser)

Use `agent-browser` via Bash for browser verification of frontend changes. It supports **multiple isolated sessions**, making it safe for parallel agent work. Do NOT use Playwright MCP — it only supports one browser at a time which conflicts with multiple agents.

**Always use `--headed --session <name>`** so the user can watch and sessions don't conflict.

```bash
# Open a page
agent-browser --headed --session wicke open http://localhost:3000

# Core workflow: snapshot → get refs → interact → re-snapshot
agent-browser --headed --session wicke snapshot -i          # Get interactive elements with refs
agent-browser --headed --session wicke click @e1            # Click element by ref
agent-browser --headed --session wicke fill @e2 "search"    # Fill input by ref
agent-browser --headed --session wicke screenshot page.png  # Take screenshot
agent-browser --headed --session wicke close                # Close when done
```

**Key commands:**
- `open <url>` — Navigate to page
- `snapshot -i` — Get interactive elements with refs like `@e1`, `@e2`
- `click @e1` — Click element by ref
- `fill @e1 "text"` — Fill input field
- `screenshot [path]` — Take screenshot
- `get text @e1` — Extract element text
- `wait --text "Success"` — Wait for text to appear
- `close` — Close the browser session

**Rules:**
1. Use a unique session name per agent task (e.g., `--session card-test`, `--session verify-ui`)
2. Snapshot before interacting — refs change after navigation
3. Re-snapshot after page changes
4. **Always close sessions when done** — orphaned sessions waste resources

**Token-efficient pattern for teams:** Spawn a single agent to handle browser automation rather than doing it inline. Keep browser interaction out of the main context.

## Security & Config Tips
- Use `.env` for local overrides; never commit secrets. Key envs: `FRONTEND_PORT`, `BACKEND_PORT`, `HOST`
- Dev ports and assets are managed by `scripts/setup-dev-environment.js`.
