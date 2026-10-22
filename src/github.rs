//! GitHub CLI backend: no tokens in memory or local config; all arguments are passed without a shell.
use crate::csv_import::{valid_repository, ImportRow};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Debug, Default)]
pub struct Project {
    pub number: u64,
    pub closed: bool,
    pub title: String,
    pub id: String,
    pub url: String,
}
#[derive(Clone, Debug, Default)]
pub struct OptionValue {
    pub name: String,
    pub id: String,
}
#[derive(Clone, Debug, Default)]
pub struct Field {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub options: Vec<OptionValue>,
}
#[derive(Clone, Debug, Default)]
pub struct Item {
    pub id: String,
    pub title: String,
    pub url: String,
    pub body: String,
    pub state: String,
    pub issue_type: String,
    pub fields: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Default)]
pub struct Board {
    pub project: Project,
    pub items: Vec<Item>,
    pub fields: Vec<Field>,
    pub compatibility_warning: Option<String>,
}
#[derive(Clone, Debug, Default)]
pub struct ImportReport {
    pub created: usize,
    pub reused: usize,
    pub added: usize,
    pub errors: Vec<String>,
}

/// Optional import behaviours, kept together to avoid hard-to-read bool arguments.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ImportOptions {
    pub create_fields: bool,
    pub create_milestones: bool,
}

fn str_field<'a>(v: &'a Value, name: &str) -> &'a str {
    v.get(name).and_then(Value::as_str).unwrap_or("")
}
fn gh_binary() -> std::ffi::OsString {
    if let Some(custom) = std::env::var_os("QUARK_GH_BINARY") {
        return custom;
    }
    #[cfg(target_os = "macos")]
    for path in ["/opt/homebrew/bin/gh", "/usr/local/bin/gh"] {
        if std::path::Path::new(path).is_file() {
            return path.into();
        }
    }
    #[cfg(windows)]
    if let Some(base) = std::env::var_os("ProgramFiles") {
        let path = std::path::PathBuf::from(base)
            .join("GitHub CLI")
            .join("gh.exe");
        if path.is_file() {
            return path.into_os_string();
        }
    }
    "gh".into()
}
fn cli(args: &[String]) -> Result<String, String> {
    let bin = gh_binary();
    let out = Command::new(bin)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| {
            format!("Unable to run GitHub CLI (gh): {e}. Install gh and sign in first.")
        })?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        let message = if stderr.is_empty() { stdout } else { stderr };
        return Err(format!(
            "gh {}: {}{}",
            args.first().map(String::as_str).unwrap_or(""),
            message,
            project_permission_hint(&message)
        ));
    }
    Ok(stdout)
}
/// Give the user an actionable recovery command without exposing credential data.
fn project_permission_hint(message: &str) -> &'static str {
    let lower = message.to_ascii_lowercase();
    if lower.contains("missing required scopes") && lower.contains("project") {
        "\nGrant GitHub Projects access to the active account with: gh auth refresh -s project,read:org\nIf GH_TOKEN or GITHUB_TOKEN is set, it overrides stored gh credentials; update or unset it."
    } else {
        ""
    }
}

fn json(args: &[String]) -> Result<Value, String> {
    let data = cli(args)?;
    serde_json::from_str(&data)
        .map_err(|e| format!("Unexpected gh JSON: {e} (check GitHub CLI version)"))
}
fn args(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| (*s).to_owned()).collect()
}
fn project_args(sub: &str, number: u64, owner: &str) -> Vec<String> {
    args(&["project", sub, &number.to_string(), "--owner", owner])
}
pub fn whoami() -> Result<String, String> {
    let value = json(&args(&["api", "user"]))?;
    let login = str_field(&value, "login");
    if login.is_empty() {
        return Err("GitHub authentication returned no username".into());
    }
    Ok(login.into())
}
pub fn projects(owner: &str) -> Result<Vec<Project>, String> {
    let value = json(&args(&[
        "project", "list", "--owner", owner, "--format", "json", "--limit", "1000", "--closed",
    ]))?;
    let mut found = value
        .get("projects")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(parse_project)
        .collect::<Vec<_>>();
    found.sort_by_key(|p| p.number);
    Ok(found)
}

/// All visible org memberships for the current account. `gh api` supports
/// `--paginate` and `--jq` on older versions that do not support `--slurp`.
/// The jq expression emits one organisation login per line, across all pages.
pub fn organizations() -> Result<Vec<String>, String> {
    let raw = cli(&args(&[
        "api",
        "--paginate",
        "--jq",
        ".[].login",
        "user/orgs?per_page=100",
    ]))?;
    Ok(parse_organizations(&raw))
}

fn parse_organizations(raw: &str) -> Vec<String> {
    let mut names: Vec<String> = raw
        .lines()
        .map(str::trim)
        .filter(|login| {
            !login.is_empty()
                && login.len() <= 39
                && !login.starts_with('-')
                && !login.ends_with('-')
                && login.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
        .map(str::to_owned)
        .collect();
    names.sort_by_key(|name| name.to_ascii_lowercase());
    names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    names
}

pub fn all_projects(
    user: &str,
    orgs: &[String],
    mut notify: impl FnMut(String),
) -> (Vec<crate::filters::OwnedProject>, Vec<String>) {
    let mut owned = Vec::new();
    let mut errors = Vec::new();
    let mut seen = HashSet::new();
    for owner in std::iter::once(user.to_owned()).chain(orgs.iter().cloned()) {
        if !seen.insert(owner.to_lowercase()) {
            continue;
        }
        notify(format!("Loading Projects for {owner}"));
        match projects(&owner) {
            Ok(values) => {
                owned.extend(
                    values
                        .into_iter()
                        .map(|project| crate::filters::OwnedProject {
                            owner: owner.clone(),
                            project,
                        }),
                )
            }
            Err(e) => errors.push(format!("{owner}: {e}")),
        }
    }
    owned.sort_by_key(|p| (p.owner.to_lowercase(), p.project.number));
    (owned, errors)
}

fn parse_project(value: &Value) -> Option<Project> {
    let number = value.get("number")?.as_u64()?;
    Some(Project {
        number,
        closed: value
            .get("closed")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        title: str_field(value, "title").into(),
        id: str_field(value, "id").into(),
        url: str_field(value, "url").into(),
    })
}
pub fn create_project(owner: &str, title: &str) -> Result<Project, String> {
    if title.trim().is_empty() {
        return Err("Enter a project name".into());
    }
    let value = json(&args(&[
        "project", "create", "--owner", owner, "--title", title, "--format", "json",
    ]))?;
    parse_project(&value)
        .ok_or_else(|| "GitHub created a project but did not return its number".into())
}
pub fn fields(owner: &str, number: u64) -> Result<Vec<Field>, String> {
    let mut a = project_args("field-list", number, owner);
    a.extend(args(&["--format", "json", "--limit", "100"]));
    let val = json(&a)?;
    Ok(parse_fields(&val))
}
fn parse_fields(v: &Value) -> Vec<Field> {
    v.get("fields")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|f| Field {
            id: str_field(f, "id").into(),
            name: str_field(f, "name").into(),
            kind: if str_field(f, "dataType").is_empty() {
                str_field(f, "type").into()
            } else {
                str_field(f, "dataType").into()
            },
            options: f
                .get("options")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|o| OptionValue {
                    id: str_field(o, "id").into(),
                    name: str_field(o, "name").into(),
                })
                .collect(),
        })
        .filter(|f| !f.name.is_empty())
        .collect()
}
fn field_value(v: &Value) -> String {
    if let Some(s) = v.as_str() {
        return s.to_owned();
    }
    if let Some(n) = v.as_f64() {
        return n.to_string();
    }
    if let Some(b) = v.as_bool() {
        return b.to_string();
    }
    str_field(v, "name").to_owned()
}
fn parse_item(v: &Value) -> Item {
    let content = v.get("content").unwrap_or(&Value::Null);
    let mut fields = BTreeMap::new();
    if let Some(map) = v.as_object() {
        for (k, val) in map {
            if !matches!(
                k.as_str(),
                "id" | "title" | "url" | "content" | "type" | "body" | "state"
            ) {
                let s = field_value(val);
                if !s.is_empty() {
                    fields.insert(k.to_owned(), s);
                }
            }
        }
    }
    let status = str_field(v, "status");
    if !status.is_empty() {
        fields.insert("Status".into(), status.into());
    }
    Item {
        id: str_field(v, "id").into(),
        title: str_field(v, "title").to_owned(),
        url: if str_field(content, "url").is_empty() {
            str_field(v, "url").into()
        } else {
            str_field(content, "url").into()
        },
        body: if str_field(content, "body").is_empty() {
            str_field(v, "body").into()
        } else {
            str_field(content, "body").into()
        },
        state: str_field(content, "state").into(),
        issue_type: str_field(v, "type").into(),
        fields,
    }
}
/// Older gh releases support `item-list --format json` but not `--field`.
/// Attempt custom columns first, and retry *only* when gh reports that flag
/// as unsupported. Other errors (including authentication failures) propagate.
fn unsupported_item_field_flag(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("unknown flag: --field")
        || lower.contains("unknown flag: '--field'")
        || lower.contains("unknown flag: \"--field\"")
}

fn item_list_args(owner: &str, number: u64, fields: &[Field], with_extra: bool) -> Vec<String> {
    let mut a = project_args("item-list", number, owner);
    a.extend(args(&["--format", "json", "--limit", "10000"]));
    if with_extra {
        for f in fields {
            if !f.name.eq_ignore_ascii_case("Title") && !f.id.is_empty() {
                a.extend(args(&["--field", &f.name]));
            }
        }
    }
    a
}

/// The injected request function lets us verify the version-compatibility path
/// without network access or a real GitHub account.
fn list_items_compatible(
    owner: &str,
    number: u64,
    fields: &[Field],
    mut request: impl FnMut(&[String]) -> Result<Value, String>,
) -> Result<(Value, bool), String> {
    let basic = item_list_args(owner, number, fields, false);
    let extended = item_list_args(owner, number, fields, true);
    if basic == extended {
        return request(&basic).map(|items| (items, false));
    }
    match request(&extended) {
        Ok(items) => Ok((items, false)),
        Err(error) if unsupported_item_field_flag(&error) => {
            request(&basic).map(|items| (items, true))
        }
        Err(error) => Err(error),
    }
}

pub fn load_board(owner: &str, project: &Project) -> Result<Board, String> {
    let fields = fields(owner, project.number)?;
    let (val, legacy_cli) = list_items_compatible(owner, project.number, &fields, json)?;
    let items = val
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(parse_item)
        .collect();
    let compatibility_warning = legacy_cli.then(|| {
        "Your installed GitHub CLI cannot request custom Project columns (--field). \
         Project items are available, but Priority, Size and other custom values may be missing. \
         Update GitHub CLI to enable these columns."
            .to_owned()
    });
    Ok(Board {
        project: project.clone(),
        items,
        fields,
        compatibility_warning,
    })
}
/// Accept only canonical github.com repository issue URLs, never lookalike hosts
/// or arbitrary text that happens to contain `/issues/`.
fn valid_issue_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://github.com/") else {
        return false;
    };
    let parts: Vec<_> = rest.split('/').collect();
    parts.len() == 4
        && valid_repository(&format!("{}/{}", parts[0], parts[1]))
        && parts[2] == "issues"
        && !parts[3].is_empty()
        && parts[3].bytes().all(|c| c.is_ascii_digit())
        && parts[3].parse::<u64>().is_ok_and(|n| n > 0)
}

pub fn create_issue(
    repo: &str,
    title: &str,
    body: &str,
    issue_type: &str,
    milestone: &str,
) -> Result<String, String> {
    if !valid_repository(repo) {
        return Err(format!("Invalid repository '{repo}' (expected owner/repo)"));
    }
    if title.trim().is_empty() {
        return Err("Issue title must not be empty".into());
    }
    let mut cmd = args(&[
        "issue", "create", "--repo", repo, "--title", title, "--body", body,
    ]);
    if !issue_type.is_empty() {
        cmd.extend(args(&["--type", issue_type]));
    }
    if !milestone.is_empty() {
        cmd.extend(args(&["--milestone", milestone]));
    }
    let url = cli(&cmd)?;
    let url = url.lines().last().unwrap_or("").trim();
    if !valid_issue_url(url) {
        return Err(format!("Unexpected issue creation response: {url}"));
    }
    Ok(url.into())
}
pub fn add_issue(owner: &str, number: u64, url: &str) -> Result<String, String> {
    let mut a = project_args("item-add", number, owner);
    a.extend(args(&["--url", url, "--format", "json"]));
    let value = json(&a)?;
    Ok(str_field(&value, "id").to_owned())
}
pub fn create_draft(owner: &str, number: u64, title: &str, body: &str) -> Result<(), String> {
    let mut a = project_args("item-create", number, owner);
    a.extend(args(&[
        "--title", title, "--body", body, "--format", "json",
    ]));
    json(&a).map(|_| ())
}
pub fn update_issue(url: &str, title: &str, body: &str) -> Result<(), String> {
    if !valid_issue_url(url) {
        return Err("Only repository issues can be edited this way".into());
    }
    cli(&args(&[
        "issue", "edit", url, "--title", title, "--body", body,
    ]))
    .map(|_| ())
}
pub fn close_issue(url: &str, close: bool) -> Result<(), String> {
    if !valid_issue_url(url) {
        return Err("Select a repository issue".into());
    }
    cli(&args(&[
        "issue",
        if close { "close" } else { "reopen" },
        url,
    ]))
    .map(|_| ())
}
pub fn remove_item(owner: &str, number: u64, id: &str) -> Result<(), String> {
    let mut a = project_args("item-delete", number, owner);
    a.extend(args(&["--id", id]));
    cli(&a).map(|_| ())
}
pub fn set_field(
    board: &Board,
    field_name: &str,
    item_id: &str,
    value: &str,
) -> Result<(), String> {
    let field = board
        .fields
        .iter()
        .find(|f| f.name.eq_ignore_ascii_case(field_name))
        .ok_or_else(|| format!("Project has no field '{field_name}'"))?;
    if field.id.is_empty() || board.project.id.is_empty() {
        return Err("Missing GitHub Project/field ID".into());
    }
    let mut a = args(&[
        "project",
        "item-edit",
        "--id",
        item_id,
        "--project-id",
        &board.project.id,
        "--field-id",
        &field.id,
    ]);
    if value.is_empty() {
        a.push("--clear".into());
    } else if !field.options.is_empty() || field.kind.to_uppercase().contains("SINGLE_SELECT") {
        let opt = field
            .options
            .iter()
            .find(|o| o.name.eq_ignore_ascii_case(value))
            .ok_or_else(|| {
                format!(
                    "'{value}' is not an available option for {field_name}. Existing options: {}",
                    field
                        .options
                        .iter()
                        .map(|o| o.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })?;
        a.extend(args(&["--single-select-option-id", &opt.id]));
    } else if field.kind.to_uppercase().contains("NUMBER") {
        value
            .parse::<f64>()
            .map_err(|_| format!("'{value}' is not a number"))?;
        a.extend(args(&["--number", value]));
    } else if field.kind.to_uppercase().contains("DATE") {
        a.extend(args(&["--date", value]));
    } else {
        a.extend(args(&["--text", value]));
    }
    cli(&a).map(|_| ())
}
pub fn create_missing_fields(
    owner: &str,
    number: u64,
    entries: &[ImportRow],
    notify: &mut impl FnMut(String),
) -> Result<Vec<Field>, String> {
    let present: HashSet<String> = fields(owner, number)?
        .iter()
        .map(|x| x.name.to_lowercase())
        .collect();
    let mut all: HashSet<String> = entries
        .iter()
        .flat_map(|r| r.fields.keys().cloned())
        .collect();
    let mut names: Vec<_> = all.drain().collect();
    names.sort();
    for name in names {
        if present.contains(&name.to_lowercase()) {
            continue;
        }
        let (kind, values) = match name.as_str() {
            "Priority" => ("SINGLE_SELECT", "P0,P1,P2"),
            "Size" => ("SINGLE_SELECT", "XS,S,M,L,XL"),
            "Status" => ("SINGLE_SELECT", "Backlog,Ready,In Progress,Testing,Done"),
            _ => ("TEXT", ""),
        };
        let mut a = project_args("field-create", number, owner);
        a.extend(args(&["--name", &name, "--data-type", kind]));
        if !values.is_empty() {
            a.extend(args(&["--single-select-options", values]));
        }
        match cli(&a) {
            Ok(_) => notify(format!("Created project field {name}")),
            Err(e) => notify(format!("Could not create field {name}: {e}")),
        }
    }
    fields(owner, number)
}

fn parse_existing_issues(value: &Value) -> HashMap<String, String> {
    let mut found = HashMap::new();
    for issue in value.as_array().into_iter().flatten() {
        let title = str_field(issue, "title").trim().to_lowercase();
        let url = str_field(issue, "url");
        if !title.is_empty() && valid_issue_url(url) {
            found.entry(title).or_insert(url.to_owned());
        }
    }
    found
}
fn existing_issues(repo: &str) -> Result<HashMap<String, String>, String> {
    let value = json(&args(&[
        "issue",
        "list",
        "--repo",
        repo,
        "--state",
        "all",
        "--limit",
        "10000",
        "--json",
        "title,url",
    ]))?;
    Ok(parse_existing_issues(&value))
}
fn parse_milestone_titles(value: &Value) -> HashSet<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| str_field(v, "title").to_lowercase())
        .filter(|v| !v.is_empty())
        .collect()
}
fn milestones(repo: &str) -> Result<HashSet<String>, String> {
    let endpoint = format!("repos/{repo}/milestones?state=all&per_page=100");
    let value = json(&args(&["api", &endpoint]))?;
    Ok(parse_milestone_titles(&value))
}
fn add_milestone(repo: &str, title: &str) -> Result<(), String> {
    cli(&args(&[
        "api",
        "--method",
        "POST",
        &format!("repos/{repo}/milestones"),
        "-f",
        &format!("title={title}"),
    ]))
    .map(|_| ())
}
/// Reuses exact-title repo issues and existing project items; progress is streamed to the UI.
pub fn import(
    owner: &str,
    mut board: Board,
    entries: Vec<ImportRow>,
    default_repo: &str,
    options: ImportOptions,
    cancel: &AtomicBool,
    notify: &mut impl FnMut(String),
) -> ImportReport {
    let mut report = ImportReport::default();
    if options.create_fields {
        match create_missing_fields(owner, board.project.number, &entries, notify) {
            Ok(fields) => board.fields = fields,
            Err(e) => report.errors.push(e),
        }
    }
    let mut known_items: HashMap<String, String> = board
        .items
        .iter()
        .map(|i| (i.url.clone(), i.id.clone()))
        .collect();
    let mut repos: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut known_milestones: HashMap<String, HashSet<String>> = HashMap::new();
    for (idx, row) in entries.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            notify("Import cancelled; completed rows were kept".into());
            break;
        }
        let repo = if row.repository.is_empty() {
            default_repo
        } else {
            &row.repository
        };
        if !valid_repository(repo) {
            report.errors.push(format!(
                "Row {} '{}' has no valid repository",
                idx + 1,
                row.title
            ));
            continue;
        }
        if !repos.contains_key(repo) {
            match existing_issues(repo) {
                Ok(v) => {
                    repos.insert(repo.to_owned(), v);
                }
                Err(e) => {
                    report
                        .errors
                        .push(format!("Cannot list {repo} issues: {e}"));
                    continue;
                }
            }
        }
        let title_key = row.title.trim().to_lowercase();
        let existing = repos.get(repo).and_then(|map| map.get(&title_key)).cloned();
        let url = if let Some(url) = existing {
            report.reused += 1;
            url
        } else {
            let mut milestone = row.milestone.as_str();
            if !milestone.is_empty() && options.create_milestones {
                if !known_milestones.contains_key(repo) {
                    match milestones(repo) {
                        Ok(v) => {
                            known_milestones.insert(repo.into(), v);
                        }
                        Err(e) => report
                            .errors
                            .push(format!("Cannot list milestones for {repo}: {e}")),
                    }
                }
                if let Some(set) = known_milestones.get_mut(repo) {
                    if !set.contains(&milestone.to_lowercase()) {
                        match add_milestone(repo, milestone) {
                            Ok(()) => {
                                set.insert(milestone.to_lowercase());
                                notify(format!("Created milestone {milestone}"));
                            }
                            Err(e) => {
                                report.errors.push(format!("Milestone '{milestone}': {e}"));
                                milestone = "";
                            }
                        }
                    }
                }
            }
            let create = create_issue(repo, &row.title, &row.body, &row.issue_type, milestone)
                .or_else(|err| {
                    // Older organizations may not have issue types; retry without type.
                    if row.issue_type.is_empty() {
                        Err(err)
                    } else {
                        notify(format!(
                            "Retrying '{}' without issue type: {err}",
                            row.title
                        ));
                        create_issue(repo, &row.title, &row.body, "", milestone)
                    }
                });
            match create {
                Ok(url) => {
                    report.created += 1;
                    repos.get_mut(repo).unwrap().insert(title_key, url.clone());
                    url
                }
                Err(e) => {
                    report
                        .errors
                        .push(format!("Row {} '{}': {e}", idx + 1, row.title));
                    continue;
                }
            }
        };
        if !known_items.contains_key(&url) {
            match add_issue(owner, board.project.number, &url) {
                Ok(id) => {
                    known_items.insert(url.clone(), id);
                    report.added += 1;
                }
                Err(e) => {
                    report
                        .errors
                        .push(format!("Cannot add '{}' to project: {e}", row.title));
                    continue;
                }
            }
        }
        if !row.fields.is_empty() {
            let mut id = known_items.get(&url).cloned().unwrap_or_default();
            if id.is_empty() {
                // Fallback for old gh versions returning no item id after linking.
                if let Ok(current) = load_board(owner, &board.project) {
                    id = current
                        .items
                        .iter()
                        .find(|i| i.url == url)
                        .map(|i| i.id.clone())
                        .unwrap_or_default();
                }
            }
            if id.is_empty() {
                report
                    .errors
                    .push(format!("Cannot resolve GitHub item ID for '{}'", row.title));
            } else {
                for (name, value) in &row.fields {
                    if let Err(e) = set_field(&board, name, &id, value) {
                        report.errors.push(format!("'{}' / {name}: {e}", row.title));
                    }
                }
            }
        }
        notify(format!("{}/{} · {}", idx + 1, entries.len(), row.title));
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_project_and_custom_fields() {
        let p =
            parse_project(&serde_json::json!({"number":3,"title":"Quark","id":"PVT_1"})).unwrap();
        assert_eq!(p.number, 3);
        let fields = parse_fields(
            &serde_json::json!({"fields":[{"name":"Priority","id":"FIELD_1","type":"ProjectV2SingleSelectField","options":[{"name":"P0","id":"OP_0"}]}]}),
        );
        assert_eq!(fields[0].options[0].name, "P0");
    }
    #[test]
    fn import_options_are_explicit_and_default_to_off() {
        let options = ImportOptions::default();
        assert!(!options.create_fields);
        assert!(!options.create_milestones);
        let custom = ImportOptions {
            create_fields: true,
            create_milestones: false,
        };
        assert_ne!(options, custom);
        assert!(custom.create_fields);
    }
    #[test]
    fn organization_list_uses_all_pages_and_deduplicates() {
        let raw = "Aurora-softwares\nOther\nAURORA-SOFTWARES\nThird\n";
        let names = parse_organizations(raw);
        assert_eq!(names.len(), 3);
        assert!(names.iter().any(|n| n == "Other"));
    }
    #[test]
    fn closed_project_is_recognized() {
        let p = parse_project(&serde_json::json!({"number":5,"title":"Archived","closed":true}))
            .unwrap();
        assert!(p.closed);
    }
    #[test]
    fn parses_items_and_status() {
        let i = parse_item(
            &serde_json::json!({"id":"PVTI_1","title":"Fix","content":{"url":"https://github.com/o/r/issues/7"},"status":"Ready","Priority":"P1"}),
        );
        assert_eq!(i.fields["Status"], "Ready");
        assert_eq!(i.fields["Priority"], "P1");
        assert!(i.url.ends_with("/7"));
    }
}

pub fn update_draft(item_id: &str, title: &str, body: &str) -> Result<(), String> {
    cli(&args(&[
        "project",
        "item-edit",
        "--id",
        item_id,
        "--title",
        title,
        "--body",
        body,
    ]))
    .map(|_| ())
}

pub fn update_project(
    owner: &str,
    number: u64,
    title: &str,
    description: &str,
) -> Result<(), String> {
    if title.trim().is_empty() {
        return Err("Project title must not be empty".into());
    }
    let mut a = project_args("edit", number, owner);
    a.extend(args(&["--title", title, "--description", description]));
    cli(&a).map(|_| ())
}
pub fn create_field(
    owner: &str,
    number: u64,
    name: &str,
    data_type: &str,
    options: &str,
) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("Field name must not be empty".into());
    }
    if !["TEXT", "NUMBER", "DATE", "SINGLE_SELECT"].contains(&data_type) {
        return Err("Invalid field type".into());
    }
    let mut a = project_args("field-create", number, owner);
    a.extend(args(&["--name", name, "--data-type", data_type]));
    if data_type == "SINGLE_SELECT" {
        if options.trim().is_empty() {
            return Err("Enter comma-separated options".into());
        }
        a.extend(args(&["--single-select-options", options]));
    }
    cli(&a).map(|_| ())
}

#[cfg(all(test, unix))]
mod mock_gh_tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};
    #[test]
    fn cli_roundtrip_auth_projects_and_create_issue() {
        let dir = tempfile::tempdir().unwrap();
        let mock = dir.path().join("gh-mock");
        fs::write(&mock, r##"#!/bin/sh
case "$1:$2" in
 api:user) printf '{"login":"local-test"}\n' ;;
 auth:status) printf '{"hosts":{"github.com":[{"login":"local-test","active":true,"state":"success"}]}}\n' ;;
 auth:switch|auth:logout) printf 'ok\n' ;;
 api:--paginate) case "$*" in *--slurp*) printf 'unsupported --slurp\n' >&2; exit 2 ;; *--jq*) printf 'Aurora\nOther\n' ;; *) exit 2 ;; esac ;;
 api:*) case "$*" in *'--method POST'*) printf '{"number":99}\n';; *) printf '[{"title":"v0.1"}]\n';; esac ;;
 project:list) printf '{"projects":[{"number":7,"title":"My board","id":"PVT_test"}]}\n' ;;
 project:create) printf '{"number":8,"title":"New board","id":"PVT_new"}\n' ;;
 project:field-list) printf '{"fields":[{"name":"Status","id":"field_1","type":"SINGLE_SELECT","options":[{"name":"Backlog","id":"opt_1"}]},{"name":"Priority","id":"field_2","type":"SINGLE_SELECT","options":[{"name":"P0","id":"opt_P0"}]},{"name":"Estimate","id":"field_3","type":"NUMBER"},{"name":"Due","id":"field_4","type":"DATE"},{"name":"Notes","id":"field_5","type":"TEXT"}]}\n' ;;
 project:item-list) case " $* " in *" --field "*) printf 'unknown flag: --field\n' >&2; exit 1;; esac
     printf '{"items":[]}\n' ;;
 project:item-add) printf '{"id":"PVTI_test"}\n' ;;
 project:item-edit|project:item-delete|project:item-create|project:field-create|project:edit) printf '{"id":"PVTI_test"}\n' ;;
 issue:list) printf '[{"title":"Already","url":"https://github.com/example/example/issues/4"}]\n' ;;
 issue:create) case "$*" in *'--type BadType'*) printf 'No such issue type\n' >&2; exit 1;; esac
     printf 'https://github.com/example/example/issues/9\n' ;;
 issue:edit|issue:close|issue:reopen) printf 'ok\n' ;;
 *) printf 'Unsupported mock command: %s %s\n' "$1" "$2" >&2; exit 1 ;;
esac
"##).unwrap();
        fs::set_permissions(&mock, fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("QUARK_GH_BINARY", &mock);
        let result = (|| -> Result<(), String> {
            assert_eq!(whoami()?, "local-test");
            assert_eq!(crate::auth::list_accounts()?.len(), 1);
            crate::auth::switch_account("local-test")?;
            crate::auth::logout("local-test")?;
            assert!(crate::auth::switch_account("--unsafe").is_err());
            let list = projects("example")?;
            assert_eq!(list[0].number, 7);
            assert_eq!(list[0].title, "My board");
            let url = create_issue("example/example", "Test", "Body", "", "")?;
            assert_eq!(url, "https://github.com/example/example/issues/9");
            let project = list[0].clone();
            let board = load_board("example", &project)?;
            assert_eq!(board.fields[0].name, "Status");
            assert!(board
                .compatibility_warning
                .as_deref()
                .unwrap_or("")
                .contains("--field"));
            let item_id = add_issue("example", 7, &url)?;
            assert_eq!(item_id, "PVTI_test");
            set_field(&board, "Status", &item_id, "Backlog")?;
            let row = ImportRow {
                title: "Create test".into(),
                body: "Hello".into(),
                repository: "example/example".into(),
                fields: BTreeMap::from([("Status".into(), "Backlog".into())]),
                ..Default::default()
            };
            let cancel = AtomicBool::new(false);
            let report = import(
                "example",
                board.clone(),
                vec![row],
                "example/example",
                ImportOptions::default(),
                &cancel,
                &mut |_| {},
            );
            assert!(report.errors.is_empty(), "{:?}", report.errors);
            assert_eq!(report.created, 1);
            assert_eq!(report.added, 1);
            // Exercise the write operations using only our temporary fake `gh` executable.
            assert_eq!(
                organizations()?,
                vec!["Aurora".to_string(), "Other".to_string()]
            );
            assert_eq!(create_project("example", "New board")?.number, 8);
            assert!(create_project("example", "   ").is_err());
            assert!(create_issue("../bad", "X", "Body", "", "").is_err());
            assert!(create_issue("example/example", "  ", "Body", "", "").is_err());
            update_project("example", 7, "New project title", "Description")?;
            assert!(update_project("example", 7, " ", "Description").is_err());
            update_draft("DRAFT_1", "Draft title", "Draft body")?;
            create_draft("example", 7, "New draft", "Draft content")?;
            update_issue(&url, "Changed", "Different body")?;
            close_issue(&url, true)?;
            close_issue(&url, false)?;
            remove_item("example", 7, &item_id)?;
            assert!(
                update_issue("https://github.com.evil.example/o/r/issues/1", "X", "Y").is_err()
            );
            assert!(close_issue("https://github.com/o/r/issues/1/evil", true).is_err());
            create_field("example", 7, "Group", "TEXT", "")?;
            create_field("example", 7, "Tier", "SINGLE_SELECT", "P0,P1,P2")?;
            assert!(create_field("example", 7, "  ", "TEXT", "").is_err());
            assert!(create_field("example", 7, "X", "INVALID", "").is_err());
            assert!(create_field("example", 7, "X", "SINGLE_SELECT", "").is_err());
            set_field(&board, "Status", &item_id, "backlog")?;
            set_field(&board, "Status", &item_id, "")?;
            set_field(&board, "Priority", &item_id, "P0")?;
            set_field(&board, "Estimate", &item_id, "42.5")?;
            set_field(&board, "Due", &item_id, "2026-10-24")?;
            set_field(&board, "Notes", &item_id, "some text")?;
            assert!(set_field(&board, "Priority", &item_id, "P9").is_err());
            assert!(set_field(&board, "Estimate", &item_id, "NaNless").is_err());
            assert!(set_field(&board, "Not a field", &item_id, "P0").is_err());
            let missing = Board {
                project: Project {
                    id: String::new(),
                    ..board.project.clone()
                },
                ..board.clone()
            };
            assert!(set_field(&missing, "Status", &item_id, "Backlog").is_err());
            let cancelled = AtomicBool::new(true);
            let no_import = import(
                "example",
                board.clone(),
                vec![ImportRow {
                    title: "Cancelled".into(),
                    repository: "example/example".into(),
                    ..Default::default()
                }],
                "example/example",
                ImportOptions::default(),
                &cancelled,
                &mut |_| {},
            );
            assert_eq!(no_import.created, 0);
            assert_eq!(no_import.added, 0);
            let bad = import(
                "example",
                board.clone(),
                vec![ImportRow {
                    title: "Bad".into(),
                    repository: "../../repo".into(),
                    ..Default::default()
                }],
                "example/example",
                ImportOptions::default(),
                &cancel,
                &mut |_| {},
            );
            assert_eq!(bad.errors.len(), 1);
            let dup = vec![
                ImportRow {
                    title: "Already".into(),
                    repository: "example/example".into(),
                    ..Default::default()
                },
                ImportRow {
                    title: "New".into(),
                    repository: "example/example".into(),
                    milestone: "v0.2".into(),
                    issue_type: "BadType".into(),
                    ..Default::default()
                },
                ImportRow {
                    title: "New".into(),
                    repository: "example/example".into(),
                    ..Default::default()
                },
            ];
            let reused = import(
                "example",
                board.clone(),
                dup,
                "example/example",
                ImportOptions {
                    create_milestones: true,
                    ..Default::default()
                },
                &cancel,
                &mut |_| {},
            );
            assert!(reused.errors.is_empty(), "{:?}", reused.errors);
            assert_eq!(reused.created, 1);
            assert_eq!(reused.reused, 2);
            assert_eq!(reused.added, 2);
            let mut events = Vec::new();
            let fields = create_missing_fields(
                "example",
                7,
                &[ImportRow {
                    title: "Add custom field".into(),
                    fields: BTreeMap::from([("Group".into(), "Diagnostics".into())]),
                    ..Default::default()
                }],
                &mut |msg| events.push(msg),
            )?;
            assert!(!fields.is_empty());
            assert!(events.iter().any(|s| s.contains("Group")));
            Ok(())
        })();
        std::env::remove_var("QUARK_GH_BINARY");
        result.unwrap();
    }
}

#[cfg(test)]
#[path = "tests/github_extended.rs"]
mod extended_tests;

#[cfg(test)]
#[path = "tests/compat_item_list.rs"]
mod compat_item_list_tests;
