use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportRow {
    pub title: String,
    pub body: String,
    pub repository: String,
    pub issue_type: String,
    pub milestone: String,
    pub fields: BTreeMap<String, String>,
}

pub fn read_csv(path: &Path) -> Result<Vec<ImportRow>, String> {
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .trim(csv::Trim::All)
        .from_path(path)
        .map_err(|e| format!("Cannot open CSV: {e}"))?;
    let headers: Vec<String> = rdr
        .headers()
        .map_err(|e| e.to_string())?
        .iter()
        .map(|h| h.trim_start_matches('\u{feff}').trim().to_owned())
        .collect();
    if !headers.iter().any(|h| h.eq_ignore_ascii_case("Title")) {
        return Err("CSV must contain a Title column".into());
    }
    let mut rows = Vec::new();
    for (i, record) in rdr.records().enumerate() {
        let record = record.map_err(|e| format!("CSV row {}: {e}", i + 2))?;
        let mut cols = BTreeMap::new();
        for (key, value) in headers.iter().zip(record.iter()) {
            cols.insert(key.to_lowercase(), value.trim().to_owned());
        }
        let get = |key: &str| cols.get(key).cloned().unwrap_or_default();
        let title = get("title");
        if title.is_empty() {
            continue;
        }
        let mut body = get("body");
        for (header, label) in [
            ("acceptance criteria", "Acceptance criteria"),
            ("depends on", "Depends on"),
        ] {
            let value = get(header);
            if !value.is_empty() && !body.contains(&value) {
                if !body.is_empty() {
                    body.push_str("\n\n");
                }
                body.push_str(&format!("{label}:\n{value}"));
            }
        }
        let mut fields = BTreeMap::new();
        for (input, name) in [
            ("status", "Status"),
            ("priority", "Priority"),
            ("size", "Size"),
            ("target version", "Target Version"),
            ("group", "Group"),
        ] {
            let v = get(input);
            if !v.is_empty() {
                fields.insert(name.into(), v);
            }
        }
        rows.push(ImportRow {
            title,
            body,
            repository: get("repository"),
            issue_type: get("type"),
            milestone: get("milestone"),
            fields,
        });
    }
    if rows.is_empty() {
        return Err("CSV has no nonempty Title rows".into());
    }
    Ok(rows)
}

pub fn valid_repository(value: &str) -> bool {
    let parts: Vec<_> = value.split('/').collect();
    parts.len() == 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && *p != "."
                && *p != ".."
                && p.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_quoted_multiline_quark_csv() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("roadmap.csv");
        std::fs::write(&path, "Title,Body,Priority,Repository,Acceptance Criteria,Target Version\n\"Hello, world\",\"One\nTwo\",P0,org/repo,Done,v0.3.1\n").unwrap();
        let rows = read_csv(&path).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "Hello, world");
        assert!(rows[0].body.contains("One\nTwo"));
        assert!(rows[0].body.contains("Acceptance criteria:\nDone"));
        assert_eq!(rows[0].fields["Priority"], "P0");
    }
    #[test]
    fn rejects_csv_without_title() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.csv");
        std::fs::write(&path, "Body,Priority\nx,P1\n").unwrap();
        assert!(read_csv(&path).is_err());
    }
    #[test]
    fn validates_repository_without_shell() {
        assert!(valid_repository("Aurora-Softwares/Quark-IDE"));
        assert!(!valid_repository("../other"));
        assert!(!valid_repository("foo/bar baz"));
        assert!(!valid_repository("foo/bar/third"));
    }
    #[test]
    fn parses_bundled_quark_roadmap() {
        let file = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("examples/Quark-IDE-Roadmap-GitHub-Import.csv");
        let rows = read_csv(&file).unwrap();
        assert_eq!(rows.len(), 75);
        assert_eq!(rows[0].fields["Priority"], "P0");
        assert_eq!(rows[0].repository, "Aurora-softwares/Quark-IDE");
        assert!(!rows[0].milestone.is_empty());
    }
}

#[cfg(test)]
#[path = "tests/csv_extended.rs"]
mod extended_tests;
