#!/usr/bin/env python3
"""
Rebrand script: Wicke Kanban -> Wickeban
Replaces all branding variants in file contents and renames files.
"""

import os
import re
import sys

# Root of the worktree
ROOT = os.path.dirname(os.path.abspath(__file__))

# Directories to skip
SKIP_DIRS = {"node_modules", "target", ".git", "dist", ".turbo", ".next"}

# Binary extensions to skip for content replacement
BINARY_EXTS = {
    ".png", ".jpg", ".jpeg", ".gif", ".ico", ".webp", ".woff", ".woff2",
    ".ttf", ".eot", ".otf", ".zip", ".tar", ".gz", ".bz2", ".pdf",
    ".mp3", ".mp4", ".mov", ".avi", ".exe", ".dll", ".so", ".dylib",
}

# Files to skip entirely
SKIP_FILES = {"rebrand.py", ".git"}

# Content replacements (order matters — longer/more specific first)
CONTENT_REPLACEMENTS = [
    # Spaced title case
    ("Wicke Kanban", "Wickeban"),
    ("Vibe Kanban", "Wickeban"),
    # PascalCase
    ("WickeKanban", "Wickeban"),
    ("VibeKanban", "Wickeban"),
    # kebab-case
    ("wicke-kanban", "wickeban"),
    ("vibe-kanban", "wickeban"),
    # snake_case
    ("wicke_kanban", "wickeban"),
    ("vibe_kanban", "wickeban"),
]

# File/directory rename replacements
FILE_RENAMES = [
    ("wicke-kanban", "wickeban"),
    ("wicke_kanban", "wickeban"),
    ("vibe-kanban", "wickeban"),
    ("vibe_kanban", "wickeban"),
]


def should_skip_dir(dirname: str) -> bool:
    return dirname in SKIP_DIRS


def is_binary(filepath: str) -> bool:
    _, ext = os.path.splitext(filepath)
    return ext.lower() in BINARY_EXTS


def replace_contents(filepath: str, dry_run: bool = False) -> bool:
    """Replace branding strings in a file's contents. Returns True if changed."""
    if is_binary(filepath):
        return False

    try:
        with open(filepath, "r", encoding="utf-8", errors="ignore") as f:
            content = f.read()
    except (OSError, UnicodeDecodeError):
        return False

    original = content
    for old, new in CONTENT_REPLACEMENTS:
        content = content.replace(old, new)

    if content != original:
        if not dry_run:
            with open(filepath, "w", encoding="utf-8") as f:
                f.write(content)
        return True
    return False


def collect_renames(root_dir: str) -> list[tuple[str, str]]:
    """Collect all files/dirs that need renaming (deepest first for safe renaming)."""
    renames = []
    for dirpath, dirnames, filenames in os.walk(root_dir):
        # Skip excluded directories
        dirnames[:] = [d for d in dirnames if not should_skip_dir(d)]

        for name in filenames + dirnames:
            new_name = name
            for old, new in FILE_RENAMES:
                new_name = new_name.replace(old, new)
            if new_name != name:
                old_path = os.path.join(dirpath, name)
                new_path = os.path.join(dirpath, new_name)
                renames.append((old_path, new_path))

    # Sort by depth (deepest first) so we rename children before parents
    renames.sort(key=lambda x: x[0].count(os.sep), reverse=True)
    return renames


def main():
    dry_run = "--dry-run" in sys.argv
    verbose = "--verbose" in sys.argv or "-v" in sys.argv

    if dry_run:
        print("=== DRY RUN (no changes will be made) ===\n")

    # Phase 1: Replace file contents
    print("Phase 1: Replacing file contents...")
    changed_files = 0
    for dirpath, dirnames, filenames in os.walk(ROOT):
        dirnames[:] = [d for d in dirnames if not should_skip_dir(d)]
        for filename in filenames:
            if filename in SKIP_FILES:
                continue
            filepath = os.path.join(dirpath, filename)
            if replace_contents(filepath, dry_run=dry_run):
                relpath = os.path.relpath(filepath, ROOT)
                changed_files += 1
                if verbose:
                    print(f"  Updated: {relpath}")

    print(f"  {changed_files} file(s) updated.\n")

    # Phase 2: Rename files and directories
    print("Phase 2: Renaming files and directories...")
    renames = collect_renames(ROOT)
    for old_path, new_path in renames:
        rel_old = os.path.relpath(old_path, ROOT)
        rel_new = os.path.relpath(new_path, ROOT)
        print(f"  Rename: {rel_old} -> {rel_new}")
        if not dry_run:
            os.rename(old_path, new_path)

    print(f"  {len(renames)} rename(s) performed.\n")

    print("Done! Rebrand complete." if not dry_run else "Done! Dry run complete.")


if __name__ == "__main__":
    main()
