use std::path::Path;

use tokio::process::Command;

/// Check if the `herd` CLI is available on the system.
pub async fn is_herd_available() -> bool {
    Command::new("herd")
        .arg("--version")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Detect the Herd `.test` URL for a given worktree path.
///
/// Runs `herd links` and parses the output to find a linked site whose
/// path matches `worktree_path` (or any subdirectory within it).
/// Returns the URL (e.g. `http://my-site.test`) if found.
pub async fn detect_herd_url(worktree_path: &Path) -> Option<String> {
    let output = Command::new("herd").arg("links").output().await.ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_herd_links_output(&stdout, worktree_path)
}

/// Parse the output of `herd links` to find a site linked to the given path.
///
/// Herd links output format (table with pipe-delimited columns):
/// ```text
/// +------+-----+----------------------+---------------------------+-------------+----+
/// | Site | SSL | URL                  | Path                      | PHP Version |    |
/// +------+-----+----------------------+---------------------------+-------------+----+
/// | name |     | http://name.test     | /path/to/site             | 8.4         | .. |
/// | name |  X  | https://name.test    | /path/to/site             | 8.4         | .. |
/// +------+-----+----------------------+---------------------------+-------------+----+
/// ```
///
/// We look for rows where the Path column matches the worktree path.
fn parse_herd_links_output(output: &str, worktree_path: &Path) -> Option<String> {
    for line in output.lines() {
        let line = line.trim();

        // Skip separator lines and header
        if line.starts_with('+') || !line.starts_with('|') {
            continue;
        }

        // Split by '|' and extract columns
        let parts: Vec<&str> = line.split('|').map(|s| s.trim()).collect();

        // Need at least: empty | Site | SSL | URL | Path | ...
        if parts.len() < 5 {
            continue;
        }

        let site_name = parts[1];
        let url = parts[3];
        let site_path = parts[4];

        // Skip header row
        if site_name == "Site" || site_path == "Path" {
            continue;
        }

        if site_name.is_empty() || site_path.is_empty() {
            continue;
        }

        // Check if the linked path matches the worktree path or is within it.
        // Uses Path::starts_with which checks path component boundaries
        // (not substring matching), so /foo won't match /foobar.
        let site_path_obj = Path::new(site_path);
        if site_path_obj == worktree_path
            || site_path_obj.starts_with(worktree_path)
            || worktree_path.starts_with(site_path_obj)
        {
            // Use the URL column directly if available, otherwise construct from site name
            if !url.is_empty() && url.starts_with("http") {
                return Some(url.to_string());
            }
            return Some(format!("http://{}.test", site_name));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn test_parse_real_herd_links_output() {
        let output = r#"
+-------------------------------------------------+-----+--------------------------------------------------------------+-------------------------------------------------------------------------------------------------+-------------+----------+
| Site                                            | SSL | URL                                                          | Path                                                                                            | PHP Version |          |
+-------------------------------------------------+-----+--------------------------------------------------------------+-------------------------------------------------------------------------------------------------+-------------+----------+
| maisonneptune                                   |     | http://maisonneptune.test                                    | /Users/dev/Projects/maisonneptune                                                               | 8.4         | v22.14.0 |
| maisonneptune-feature_careers_page              |     | http://maisonneptune-feature_careers_page.test               | /Users/dev/Projects/maisonneptune-worktrees/feature_careers_page                                | 8.4         | v22.14.0 |
| pokemontcg-fix_shop_set_filter                  |  X  | https://pokemontcg-fix_shop_set_filter.test                  | /Users/dev/Projects/PokemonTCG-worktrees/fix_shop_set_filter                                    | 8.4         | 22       |
+-------------------------------------------------+-----+--------------------------------------------------------------+-------------------------------------------------------------------------------------------------+-------------+----------+
"#;

        // Match by worktree path
        let path =
            PathBuf::from("/Users/dev/Projects/maisonneptune-worktrees/feature_careers_page");
        let result = parse_herd_links_output(output, &path);
        assert_eq!(
            result,
            Some("http://maisonneptune-feature_careers_page.test".to_string())
        );

        // Match HTTPS site
        let path = PathBuf::from("/Users/dev/Projects/PokemonTCG-worktrees/fix_shop_set_filter");
        let result = parse_herd_links_output(output, &path);
        assert_eq!(
            result,
            Some("https://pokemontcg-fix_shop_set_filter.test".to_string())
        );

        // Match main repo
        let path = PathBuf::from("/Users/dev/Projects/maisonneptune");
        let result = parse_herd_links_output(output, &path);
        assert_eq!(result, Some("http://maisonneptune.test".to_string()));
    }

    #[test]
    fn test_parse_herd_links_no_match() {
        let output = r#"
+-----------+-----+---------------------------+----------------------------------+-------------+----+
| Site      | SSL | URL                       | Path                             | PHP Version |    |
+-----------+-----+---------------------------+----------------------------------+-------------+----+
| other-app |     | http://other-app.test     | /Users/dev/Projects/other-app    | 8.4         | .. |
+-----------+-----+---------------------------+----------------------------------+-------------+----+
"#;

        let path = PathBuf::from("/Users/dev/worktrees/feature-branch");
        let result = parse_herd_links_output(output, &path);
        assert_eq!(result, None);
    }
}
