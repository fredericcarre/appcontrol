use std::process::Command;

fn main() {
    // Embed git commit hash at compile time
    let git_hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=GIT_HASH={}", git_hash.trim());

    // Embed build timestamp (UTC)
    let output = Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=BUILD_TIME={}", output.trim());

    // Embed SQLite migration files into the binary at compile time.
    // This makes the standalone Windows .exe fully self-contained — no need to
    // ship migrations/ directory alongside the binary.
    let sqlite_migrations_dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/sqlite");

    if sqlite_migrations_dir.exists() {
        let mut entries: Vec<(i32, String, std::path::PathBuf)> = Vec::new();

        for entry in std::fs::read_dir(&sqlite_migrations_dir).unwrap() {
            let entry = entry.unwrap();
            let filename = entry.file_name().to_string_lossy().to_string();
            if filename.ends_with(".sql") && filename.starts_with('V') {
                if let Some(version_str) = filename
                    .strip_prefix('V')
                    .and_then(|s| s.split("__").next())
                {
                    if let Ok(version) = version_str.parse::<i32>() {
                        entries.push((version, filename, entry.path()));
                    }
                }
            }
        }

        entries.sort_by_key(|(v, _, _)| *v);

        // Generate a Rust source file with embedded migrations
        let out_dir = std::env::var("OUT_DIR").unwrap();
        let dest_path = std::path::Path::new(&out_dir).join("embedded_sqlite_migrations.rs");

        let mut code = String::new();
        code.push_str("/// SQLite migrations embedded at compile time.\n");
        code.push_str("/// Each entry is (version, filename, sql_content).\n");
        code.push_str("pub const MIGRATIONS: &[(i32, &str, &str)] = &[\n");

        for (version, filename, path) in &entries {
            // Use include_str! would be ideal but we need to generate the path dynamically.
            // Instead, read the file content and embed it as a string literal.
            let content = std::fs::read_to_string(path)
                .unwrap_or_else(|e| panic!("Failed to read migration {}: {}", filename, e));
            // Escape for Rust raw string literal
            code.push_str(&format!(
                "    ({}, \"{}\", r####\"{}\"####),\n",
                version, filename, content
            ));
        }

        code.push_str("];\n");

        std::fs::write(&dest_path, code).unwrap();

        // Re-run if any migration file changes
        println!("cargo:rerun-if-changed={}", sqlite_migrations_dir.display());
        for entry in std::fs::read_dir(&sqlite_migrations_dir).unwrap() {
            let entry = entry.unwrap();
            println!("cargo:rerun-if-changed={}", entry.path().display());
        }
    }

    // Only re-run if git HEAD changes
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs/");
}
