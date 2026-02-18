# Tier 2 — Isolated Worktree Environment

## Trigger

Use this skill when starting any task that involves:
- Feature development with database changes
- Running migrations or modifying schema
- Testing with data that could conflict with other work
- Any task where isolation is important

## Workflow

### 1. Ask for Branch Name

Ask the user what branch name to use. Suggest a conventional name based on the task:
- `feature/<name>` for new features
- `fix/<name>` for bug fixes
- `refactor/<name>` for refactoring
- `test/<name>` for testing/experiments

### 2. Create the Worktree

Run from the **main project root**:

```bash
bin/worktree create <branch-name>
```

This will:
- Create a git worktree at `../wickeban-worktrees/<branch-name>`
- Copy `dev_assets_seed/` to the worktree's `dev_assets/` (isolated SQLite DB)
- Allocate unique development ports
- Run `pnpm install`

For frontend-only work (shared database), use:
```bash
bin/worktree create <branch-name> --no-db
```

### 3. Verify the Environment

```bash
bin/worktree info <branch-name>
```

Confirm:
- Path exists
- Database status (isolated or shared)
- Ports are allocated
- Dependencies are installed

### 4. Update Context

Tell the user the worktree details:

```
Worktree ready!

Branch:   <branch-name>
Path:     ../wickeban-worktrees/<branch-name>
Frontend: http://localhost:<port>
Backend:  http://localhost:<port>
Database: isolated (copied from seed)

To start developing:
  cd ../wickeban-worktrees/<branch-name>
  pnpm run dev
```

### 5. Rename Tab (Optional)

If `~/.claude/scripts/set-title.sh` exists, rename the terminal tab:

```bash
~/.claude/scripts/set-title.sh "wickeban: <branch-name>"
```

### 6. Start Working

Change to the worktree directory and begin development. The worktree has its own:
- Git branch (commits won't affect main)
- SQLite database (data changes are isolated)
- Port allocation (no conflicts with main dev server)
- `node_modules/` (independent dependencies)

### 7. Cleanup

When done, return to the main project and remove the worktree:

```bash
bin/worktree remove <branch-name>
```

## Notes

- Always run `bin/worktree` commands from the **main project root**, not from inside a worktree
- Each worktree auto-allocates unique ports via `scripts/setup-dev-environment.js`
- The Rust `target/` directory is per-worktree — first build in a new worktree will be slower
- Use `bin/worktree list` to see all active worktrees and their URLs
