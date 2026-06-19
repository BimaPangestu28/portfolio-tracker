use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;

pub type Db = SqlitePool;

pub async fn connect(url: &str) -> anyhow::Result<Db> {
    let opts = SqliteConnectOptions::from_str(url)?
        .create_if_missing(true)
        .pragma("foreign_keys", "ON");
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    /// Guards against concurrent-merge collisions: two branches each adding a
    /// migration with the same numeric version (e.g. both pick `0028_`) compile
    /// fine in isolation but break startup once merged — sqlx rejects a
    /// duplicate `_sqlx_migrations.version`. This fails fast with the offending
    /// files instead of a cryptic UNIQUE-constraint error at boot.
    #[test]
    fn migration_versions_are_unique() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/migrations");
        let mut by_version: HashMap<String, Vec<String>> = HashMap::new();
        for entry in std::fs::read_dir(dir).expect("read migrations dir") {
            let name = entry.expect("dir entry").file_name().to_string_lossy().into_owned();
            if !name.ends_with(".sql") {
                continue;
            }
            // Version is the leading numeric prefix before the first '_'.
            let version = name.split('_').next().unwrap_or("").to_string();
            by_version.entry(version).or_default().push(name);
        }
        let dups: Vec<_> = by_version.values().filter(|files| files.len() > 1).collect();
        assert!(
            dups.is_empty(),
            "duplicate migration version numbers found (rename one): {dups:?}"
        );
    }
}
