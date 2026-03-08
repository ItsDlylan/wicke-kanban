/**
 * Preset script templates for common project types.
 * Each preset auto-fills setup, cleanup, archive, and copy_files fields.
 */

export interface RepoPreset {
  id: string;
  /** i18n key suffix under settings.repos.scripts.preset */
  labelKey: string;
  descriptionKey: string;
  setup_script: string;
  cleanup_script: string;
  archive_script: string;
  copy_files: string;
  dev_server_script: string;
}

/**
 * Laravel + Herd preset.
 *
 * Assumes:
 * - PostgreSQL database (configurable via env vars)
 * - Herd CLI available for `.test` domain linking
 * - Composer + npm for dependency management
 * - .env file in the main repo (copied via copy_files)
 *
 * The setup script derives a site name and DB name from the directory name,
 * which Wicke sets to the repo name within the workspace container.
 */
const LARAVEL_HERD_SETUP = `#!/bin/bash
set -e

# --- Configuration ---
# Override these with environment variables if needed
DB_USER="\${DB_USER:-$(whoami)}"
DB_PASSWORD="\${DB_PASSWORD:-secret}"
DB_HOST="\${DB_HOST:-127.0.0.1}"
DB_PORT="\${DB_PORT:-5432}"

# Derive names from the worktree directory
REPO_NAME=$(basename "$(git rev-parse --show-toplevel 2>/dev/null || pwd)")
PARENT_REPO=$(basename "$(dirname "$(pwd)")")
BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")
SANITIZED_BRANCH=$(echo "$BRANCH" | tr '[:upper:]' '[:lower:]' | sed 's/[^a-z0-9]/_/g' | sed 's/__*/_/g' | sed 's/^_//;s/_$//')

SITE_NAME="\${PARENT_REPO}-\${SANITIZED_BRANCH}"
DB_NAME="\${PARENT_REPO}_\${SANITIZED_BRANCH}"

echo "Setting up Laravel environment..."
echo "  Site: http://\${SITE_NAME}.test"
echo "  Database: \${DB_NAME}"

# Create database if it doesn't exist
if ! PGPASSWORD="$DB_PASSWORD" psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -lqt 2>/dev/null | cut -d \\| -f 1 | grep -qw "$DB_NAME"; then
  echo "Creating database \${DB_NAME}..."
  PGPASSWORD="$DB_PASSWORD" createdb -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" "$DB_NAME" 2>/dev/null || true
fi

# Update .env (copied via copy_files)
if [ -f .env ]; then
  sed -i '' "s|^DB_DATABASE=.*|DB_DATABASE=$DB_NAME|" .env
  sed -i '' "s|^APP_URL=.*|APP_URL=http://\${SITE_NAME}.test|" .env
  sed -i '' "s|^SESSION_DOMAIN=.*|SESSION_DOMAIN=\${SITE_NAME}.test|" .env 2>/dev/null || true
fi

# Link in Herd
herd link "$SITE_NAME" 2>/dev/null || echo "Warning: Could not link site in Herd"

# Install dependencies
composer install --quiet --no-interaction 2>/dev/null || composer install --no-interaction
npm install --silent 2>/dev/null || npm install

# Run migrations
php artisan migrate --force --no-interaction 2>/dev/null || true

echo "Setup complete!"`;

const LARAVEL_HERD_CLEANUP = `#!/bin/bash
set -e

echo "Running post-agent quality checks..."

# Format code with Pint (Laravel's code style fixer)
if [ -f vendor/bin/pint ]; then
  echo "Running Pint..."
  ./vendor/bin/pint --quiet 2>/dev/null || ./vendor/bin/pint
fi

# Run tests
echo "Running tests..."
php artisan test --no-interaction 2>/dev/null || true

echo "Cleanup checks complete!"`;

const LARAVEL_HERD_ARCHIVE = `#!/bin/bash
set -e

DB_USER="\${DB_USER:-$(whoami)}"
DB_PASSWORD="\${DB_PASSWORD:-secret}"
DB_HOST="\${DB_HOST:-127.0.0.1}"
DB_PORT="\${DB_PORT:-5432}"

PARENT_REPO=$(basename "$(dirname "$(pwd)")")
BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")
SANITIZED_BRANCH=$(echo "$BRANCH" | tr '[:upper:]' '[:lower:]' | sed 's/[^a-z0-9]/_/g' | sed 's/__*/_/g' | sed 's/^_//;s/_$//')

SITE_NAME="\${PARENT_REPO}-\${SANITIZED_BRANCH}"
DB_NAME="\${PARENT_REPO}_\${SANITIZED_BRANCH}"

echo "Archiving Laravel workspace..."

# Unlink from Herd
herd unsecure "$SITE_NAME" 2>/dev/null || true
herd unlink "$SITE_NAME" 2>/dev/null || true

# Drop database
if PGPASSWORD="$DB_PASSWORD" psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -lqt 2>/dev/null | cut -d \\| -f 1 | grep -qw "$DB_NAME"; then
  echo "Dropping database \${DB_NAME}..."
  PGPASSWORD="$DB_PASSWORD" dropdb -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" "$DB_NAME" 2>/dev/null || true
fi

echo "Archive cleanup complete!"`;

export const REPO_PRESETS: RepoPreset[] = [
  {
    id: 'laravel-herd',
    labelKey: 'laravelHerd',
    descriptionKey: 'laravelHerdDescription',
    setup_script: LARAVEL_HERD_SETUP,
    cleanup_script: LARAVEL_HERD_CLEANUP,
    archive_script: LARAVEL_HERD_ARCHIVE,
    copy_files: '.env',
    dev_server_script: '',
  },
];
