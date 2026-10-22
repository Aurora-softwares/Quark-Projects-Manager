#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
mod auth;
mod csv_import;
mod filters;
mod github;

use auth::{Account, DeviceCode};
use csv_import::{read_csv, ImportRow};
use eframe::egui::{self, Color32, RichText};
use filters::{OwnedProject, Scope};
use github::{Board, Field, Item, Project};
use std::{
    path::PathBuf,
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    thread,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    AllProjects,
    Projects,
    Issues,
    Import,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Table,
    Board,
}

enum Outcome {
    Auth(String),
    Projects(Vec<Project>),
    Loaded(Board),
    CreatedProject(Project),
    Completed(String),
    Imported(github::ImportReport),
    Discovered {
        accounts: Vec<Account>,
        orgs: Vec<String>,
        entries: Vec<OwnedProject>,
        warnings: Vec<String>,
    },
    AccountChanged(String),
    AccountRemoved(String),
}
enum Event {
    Progress(String),
    Device(DeviceCode),
    Complete(Result<Outcome, String>),
}

struct DesktopApp {
    tx: Sender<Event>,
    rx: Receiver<Event>,
    cancel: Arc<AtomicBool>,
    owner: String,
    current_user: String,
    repo: String,
    projects: Vec<Project>,
    selected_project: Option<u64>,
    board: Option<Board>,
    selected_item: Option<String>,
    page: Page,
    view: View,
    filter: String,
    accounts: Vec<Account>,
    account_choice: String,
    show_accounts: bool,
    logout_confirm: bool,
    oauth_client_id: String,
    device: Option<DeviceCode>,
    orgs: Vec<String>,
    all_projects: Vec<OwnedProject>,
    discovery_warnings: Vec<String>,
    project_search: String,
    project_owner_filter: String,
    project_scope: Scope,
    show_closed: bool,
    item_status: String,
    item_priority: String,
    item_size: String,
    project_title: String,
    project_edit_title: String,
    project_description: String,
    new_field_name: String,
    new_field_type: String,
    new_field_options: String,
    issue_title: String,
    issue_body: String,
    issue_type: String,
    issue_milestone: String,
    draft_issue: bool,
    edit_title: String,
    edit_body: String,
    field_name: String,
    field_value: String,
    confirm_delete: bool,
    csv_path: String,
    csv_rows: Vec<ImportRow>,
    create_fields: bool,
    create_milestones: bool,
    busy: bool,
    busy_label: String,
    log: Vec<String>,
    error: String,
}
impl DesktopApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let mut app = Self::empty();
        if let Some(arg) = std::env::args().nth(1) {
            app.load_csv(PathBuf::from(arg));
            app.page = Page::Import;
        }
        app.check_auth();
        app
    }
    /// Constructs deterministic, disconnected UI state for headless tests.
    fn empty() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx,
            rx,
            cancel: Arc::new(AtomicBool::new(false)),
            owner: "Aurora-softwares".into(),
            current_user: String::new(),
            repo: "Aurora-softwares/Quark-IDE".into(),
            projects: vec![],
            selected_project: None,
            board: None,
            selected_item: None,
            page: Page::AllProjects,
            view: View::Table,
            filter: String::new(),
            accounts: vec![],
            account_choice: String::new(),
            show_accounts: false,
            logout_confirm: false,
            oauth_client_id: std::env::var("QUARK_GITHUB_CLIENT_ID").unwrap_or_default(),
            device: None,
            orgs: vec![],
            all_projects: vec![],
            discovery_warnings: vec![],
            project_search: String::new(),
            project_owner_filter: String::new(),
            project_scope: Scope::All,
            show_closed: false,
            item_status: String::new(),
            item_priority: String::new(),
            item_size: String::new(),
            project_title: String::new(),
            project_edit_title: String::new(),
            project_description: String::new(),
            new_field_name: String::new(),
            new_field_type: "TEXT".into(),
            new_field_options: String::new(),
            issue_title: String::new(),
            issue_body: String::new(),
            issue_type: String::new(),
            issue_milestone: String::new(),
            draft_issue: false,
            edit_title: String::new(),
            edit_body: String::new(),
            field_name: String::new(),
            field_value: String::new(),
            confirm_delete: false,
            csv_path: String::new(),
            csv_rows: vec![],
            create_fields: true,
            create_milestones: true,
            busy: false,
            busy_label: String::new(),
            log: vec![],
            error: String::new(),
        }
    }
    fn launch<F>(&mut self, label: &str, work: F)
    where
        F: FnOnce(Sender<Event>, Arc<AtomicBool>) -> Result<Outcome, String> + Send + 'static,
    {
        if self.busy {
            return;
        }
        self.busy = true;
        self.busy_label = label.into();
        self.error.clear();
        self.cancel.store(false, Ordering::Relaxed);
        let tx = self.tx.clone();
        let cancel = self.cancel.clone();
        thread::spawn(move || {
            let outcome = work(tx.clone(), cancel);
            let _ = tx.send(Event::Complete(outcome));
        });
    }
    fn check_auth(&mut self) {
        self.launch("Checking GitHub sign-in", |_, _| {
            github::whoami().map(Outcome::Auth)
        });
    }
    fn discover(&mut self) {
        let user = self.current_user.clone();
        if user.is_empty() {
            return;
        }
        self.launch(
            "Finding accounts, organisations and Projects",
            move |tx, cancel| {
                let accounts = auth::list_accounts().unwrap_or_default();
                let (orgs, mut warnings) = match github::organizations() {
                    Ok(orgs) => (orgs, Vec::new()),
                    Err(e) => (Vec::new(), vec![format!("Organisation discovery: {e}")]),
                };
                let (entries, project_warnings) = github::all_projects(&user, &orgs, |msg| {
                    if !cancel.load(Ordering::Relaxed) {
                        let _ = tx.send(Event::Progress(msg));
                    }
                });
                warnings.extend(project_warnings);
                Ok(Outcome::Discovered {
                    accounts,
                    orgs,
                    entries,
                    warnings,
                })
            },
        );
    }
    fn start_oauth(&mut self) {
        let client_id = self.oauth_client_id.trim().to_owned();
        self.device = None;
        self.launch("Signing in with GitHub OAuth", move |tx, cancel| {
            let device = auth::start_device_login(&client_id)?;
            let _ = tx.send(Event::Device(device.clone()));
            let user = auth::finish_device_login(&client_id, &device, &cancel)?;
            Ok(Outcome::Auth(user))
        });
    }
    fn switch_account(&mut self) {
        let login = self.account_choice.clone();
        self.launch("Switching GitHub account", move |_, _| {
            auth::switch_account(&login)?;
            Ok(Outcome::AccountChanged(login))
        });
    }
    fn logout_account(&mut self) {
        let login = self.account_choice.clone();
        self.logout_confirm = false;
        self.launch("Signing out account", move |_, _| {
            auth::logout(&login)?;
            Ok(Outcome::AccountRemoved(login))
        });
    }
    fn open_owned_project(&mut self, project: &OwnedProject) {
        self.owner = project.owner.clone();
        self.projects = self
            .all_projects
            .iter()
            .filter(|p| p.owner.eq_ignore_ascii_case(&project.owner))
            .map(|p| p.project.clone())
            .collect();
        self.selected_project = None;
        self.board = None;
        self.selected_item = None;
        self.page = Page::Projects;
        self.load_board(project.project.number);
    }
    fn load_projects(&mut self) {
        let owner = self.owner.trim().to_owned();
        if owner.is_empty() {
            self.error = "Enter an owner (username or organization)".into();
            return;
        }
        self.launch("Loading projects", move |_, _| {
            github::projects(&owner).map(Outcome::Projects)
        });
    }
    fn load_board(&mut self, number: u64) {
        let owner = self.owner.clone();
        if let Some(p) = self.projects.iter().find(|p| p.number == number).cloned() {
            if self.selected_project != Some(number) {
                self.board = None;
                self.selected_item = None;
            }
            self.selected_project = Some(number);
            self.launch("Loading project items", move |_, _| {
                github::load_board(&owner, &p).map(Outcome::Loaded)
            });
        }
    }
    fn refresh_board(&mut self) {
        if let Some(n) = self.selected_project {
            self.load_board(n)
        } else {
            self.load_projects()
        }
    }
    fn submit_project(&mut self) {
        let owner = self.owner.clone();
        let title = self.project_title.trim().to_owned();
        self.launch("Creating project", move |_, _| {
            github::create_project(&owner, &title).map(Outcome::CreatedProject)
        });
    }
    fn submit_issue(&mut self) {
        let Some(n) = self.selected_project else {
            self.error = "Choose a Project first".into();
            return;
        };
        let owner = self.owner.clone();
        let repo = self.repo.trim().to_owned();
        let title = self.issue_title.trim().to_owned();
        let body = self.issue_body.clone();
        let issue_type = self.issue_type.trim().to_owned();
        let milestone = self.issue_milestone.trim().to_owned();
        let draft = self.draft_issue;
        if title.is_empty() {
            self.error = "Enter an issue title".into();
            return;
        }
        self.launch("Creating issue", move |_, _| {
            if draft {
                github::create_draft(&owner, n, &title, &body)?;
            } else {
                let url = github::create_issue(&repo, &title, &body, &issue_type, &milestone)?;
                let _ = github::add_issue(&owner, n, &url)?;
            }
            Ok(Outcome::Completed(format!("Created '{title}'")))
        });
    }
    fn save_project_settings(&mut self) {
        let Some(n) = self.selected_project else {
            return;
        };
        let owner = self.owner.clone();
        let title = self.project_edit_title.clone();
        let desc = self.project_description.clone();
        self.launch("Updating project", move |_, _| {
            github::update_project(&owner, n, &title, &desc)?;
            Ok(Outcome::Completed(format!("Updated project #{n}")))
        });
    }
    fn add_custom_field(&mut self) {
        let Some(n) = self.selected_project else {
            return;
        };
        let owner = self.owner.clone();
        let name = self.new_field_name.clone();
        let kind = self.new_field_type.clone();
        let options = self.new_field_options.clone();
        self.launch("Creating project field", move |_, _| {
            github::create_field(&owner, n, &name, &kind, &options)?;
            Ok(Outcome::Completed(format!("Created field {name}")))
        });
    }
    fn load_csv(&mut self, path: PathBuf) {
        match read_csv(&path) {
            Ok(rows) => {
                self.csv_rows = rows;
                self.csv_path = path.display().to_string();
                self.error.clear();
            }
            Err(err) => {
                self.csv_rows.clear();
                self.csv_path.clear();
                self.error = err;
            }
        }
    }
    fn submit_csv(&mut self) {
        let Some(b) = self.board.clone() else {
            self.error = "Select a project and load its items first".into();
            return;
        };
        if self.csv_rows.is_empty() {
            self.error = "Select a CSV file".into();
            return;
        }
        let rows = self.csv_rows.clone();
        let owner = self.owner.clone();
        let repo = self.repo.clone();
        let fields = self.create_fields;
        let milestones = self.create_milestones;
        self.launch("Importing CSV", move |tx, cancel| {
            let mut notify = |message: String| {
                let _ = tx.send(Event::Progress(message));
            };
            let result = github::import(
                &owner,
                b,
                rows,
                &repo,
                github::ImportOptions {
                    create_fields: fields,
                    create_milestones: milestones,
                },
                &cancel,
                &mut notify,
            );
            Ok(Outcome::Imported(result))
        });
    }
    fn select_item(&mut self, item: &Item) {
        self.selected_item = Some(item.id.clone());
        self.edit_title = item.title.clone();
        self.edit_body = item.body.clone();
        self.field_name.clear();
        self.field_value.clear();
        self.confirm_delete = false;
    }
    fn save_issue(&mut self) {
        let Some(item) = self.active_item() else {
            return;
        };
        let title = self.edit_title.clone();
        let body = self.edit_body.clone();
        self.launch("Saving issue", move |_, _| {
            if item.url.is_empty() {
                github::update_draft(&item.id, &title, &body)?;
            } else {
                github::update_issue(&item.url, &title, &body)?;
            }
            Ok(Outcome::Completed(format!("Updated '{title}'")))
        });
    }
    fn set_selected_field(&mut self) {
        let Some(board) = self.board.clone() else {
            return;
        };
        let Some(item) = self.active_item() else {
            return;
        };
        let field = self.field_name.clone();
        let value = self.field_value.clone();
        if field.is_empty() {
            self.error = "Choose a field".into();
            return;
        }
        self.launch("Updating project field", move |_, _| {
            github::set_field(&board, &field, &item.id, &value)?;
            Ok(Outcome::Completed(format!("Updated {field}")))
        });
    }
    fn active_item(&self) -> Option<Item> {
        self.board
            .as_ref()?
            .items
            .iter()
            .find(|i| Some(&i.id) == self.selected_item.as_ref())
            .cloned()
    }
    fn remove_selected(&mut self) {
        let Some(item) = self.active_item() else {
            return;
        };
        let Some(n) = self.selected_project else {
            return;
        };
        let owner = self.owner.clone();
        self.launch("Removing Project item", move |_, _| {
            github::remove_item(&owner, n, &item.id)?;
            Ok(Outcome::Completed(format!(
                "Removed '{}' from project (issue retained)",
                item.title
            )))
        });
    }
    fn close_selected(&mut self, close: bool) {
        let Some(item) = self.active_item() else {
            return;
        };
        self.launch("Changing issue state", move |_, _| {
            github::close_issue(&item.url, close)?;
            Ok(Outcome::Completed(format!(
                "{} issue",
                if close { "Closed" } else { "Reopened" }
            )))
        });
    }
    fn process_events(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Device(device) => {
                    self.device = Some(device);
                }
                Event::Progress(text) => {
                    self.log.push(text);
                    if self.log.len() > 1000 {
                        self.log.drain(0..200);
                    }
                }
                Event::Complete(result) => {
                    self.busy = false;
                    self.device = None;
                    match result {
                        Err(e) => {
                            self.error = e;
                        }
                        Ok(Outcome::Auth(user)) => {
                            self.current_user = user.clone();
                            self.account_choice = user.clone();
                            self.projects.clear();
                            self.board = None;
                            self.selected_project = None;
                            self.all_projects.clear();
                            self.orgs.clear();
                            self.log.push(format!("Signed in as {user}"));
                            self.discover();
                        }
                        Ok(Outcome::AccountChanged(user)) => {
                            self.log.push(format!("Active account: {user}"));
                            self.check_auth();
                        }
                        Ok(Outcome::AccountRemoved(user)) => {
                            self.log.push(format!("Signed out {user}"));
                            self.current_user.clear();
                            self.accounts.clear();
                            self.projects.clear();
                            self.all_projects.clear();
                            self.orgs.clear();
                            self.board = None;
                            self.selected_project = None;
                            self.check_auth();
                        }
                        Ok(Outcome::Discovered {
                            accounts,
                            orgs,
                            entries,
                            warnings,
                        }) => {
                            self.accounts = accounts;
                            if let Some(account) = self.accounts.iter().find(|a| a.active) {
                                self.account_choice = account.login.clone();
                            }
                            self.orgs = orgs;
                            self.all_projects = entries;
                            self.discovery_warnings = warnings;
                            self.projects = self
                                .all_projects
                                .iter()
                                .filter(|p| p.owner.eq_ignore_ascii_case(&self.owner))
                                .map(|p| p.project.clone())
                                .collect();
                            self.log.push(format!(
                                "Found {} Projects across {} organisations and personal account",
                                self.all_projects.len(),
                                self.orgs.len()
                            ));
                        }
                        Ok(Outcome::Projects(list)) => {
                            self.projects = list;
                            self.all_projects
                                .retain(|p| !p.owner.eq_ignore_ascii_case(&self.owner));
                            self.all_projects.extend(self.projects.iter().cloned().map(
                                |project| OwnedProject {
                                    owner: self.owner.clone(),
                                    project,
                                },
                            ));
                            if let Some(n) = self.selected_project {
                                if self.projects.iter().any(|p| p.number == n) {
                                    self.load_board(n);
                                } else {
                                    self.board = None;
                                    self.selected_project = None;
                                }
                            }
                        }
                        Ok(Outcome::Loaded(board)) => {
                            if self.board.as_ref().map(|b| b.project.id.as_str())
                                != Some(board.project.id.as_str())
                            {
                                self.project_edit_title = board.project.title.clone();
                                self.project_description = String::new();
                            }
                            let still_selected = self
                                .selected_item
                                .as_ref()
                                .is_some_and(|id| board.items.iter().any(|i| &i.id == id));
                            self.board = Some(board);
                            if !still_selected {
                                self.selected_item = None;
                            }
                        }
                        Ok(Outcome::CreatedProject(project)) => {
                            self.log.push(format!(
                                "Created Project #{}: {}",
                                project.number, project.title
                            ));
                            self.selected_project = Some(project.number);
                            self.project_title.clear();
                            self.load_projects();
                        }
                        Ok(Outcome::Completed(message)) => {
                            if message.starts_with("Created '") {
                                self.issue_title.clear();
                                self.issue_body.clear();
                                self.page = Page::Projects;
                            }
                            self.log.push(message);
                            self.refresh_board();
                        }
                        Ok(Outcome::Imported(report)) => {
                            let summary=format!("Import: {} created, {} reused, {} added to Project, {} warnings/errors",report.created,report.reused,report.added,report.errors.len());
                            self.log.push(summary);
                            for err in report.errors {
                                self.log.push(format!("⚠ {err}"));
                            }
                            self.refresh_board();
                        }
                    }
                }
            }
        }
    }
    fn header(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("header").show(ctx,|ui|{
            ui.horizontal(|ui|{
                ui.heading(RichText::new("◈ Quark Projects").strong().size(22.0));
                ui.separator();
                let name=if self.current_user.is_empty() {"Not signed in"}else{&self.current_user};
                ui.label(RichText::new(name).color(Color32::LIGHT_GRAY));
                if ui.button("Sign in / refresh auth").clicked() && !self.busy {
                    if let Err(e)=launch_login_terminal(){self.error=e;}else{self.log.push("GitHub sign-in terminal opened; complete login there, then click Verify sign-in".into());}
                }
                if ui.button("Verify sign-in").clicked() && !self.busy {self.check_auth();}
                if ui.button("Accounts / OAuth").clicked() {self.show_accounts = !self.show_accounts;}
                if ui.button("All Projects").clicked() {self.page=Page::AllProjects;}
                if self.busy {ui.spinner();ui.label(&self.busy_label);
                    if self.busy_label=="Importing CSV" && ui.button("Stop after current row").clicked(){self.cancel.store(true,Ordering::Relaxed);}}
            });
            if self.show_accounts {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.strong("Accounts:");
                    egui::ComboBox::from_id_salt("account_switcher")
                        .selected_text(if self.account_choice.is_empty(){"No account"}else{&self.account_choice})
                        .show_ui(ui,|ui|{
                            for account in &self.accounts {
                                let text=format!("{}{}{}",account.login,
                                    if account.active {" (active)"}else{""},
                                    if account.valid {""}else{" (authentication problem)"});
                                ui.selectable_value(&mut self.account_choice,account.login.clone(),text);
                            }
                        });
                    if ui.add_enabled(!self.busy,egui::Button::new("Switch")).clicked(){self.switch_account();}
                    if ui.add_enabled(!self.busy,egui::Button::new("Refresh accounts and organisations")).clicked(){self.discover();}
                    ui.checkbox(&mut self.logout_confirm,"Confirm sign-out");
                    if ui.add_enabled(!self.busy && self.logout_confirm,egui::Button::new("Sign out selected")).clicked(){self.logout_account();}
                });
                ui.horizontal(|ui|{
                    ui.label("OAuth App client ID:");
                    ui.add(egui::TextEdit::singleline(&mut self.oauth_client_id).desired_width(240.0));
                    if ui.add_enabled(!self.busy,egui::Button::new("Sign in with GitHub (device flow)")).clicked(){self.start_oauth();}
                    if self.device.is_some() && ui.button("Cancel").clicked(){self.cancel.store(true,Ordering::Relaxed);}
                });
                if let Some(device)=&self.device {
                    ui.horizontal_wrapped(|ui| {
                        ui.strong(format!("Code: {}",device.code));
                        if ui.button("Copy code").clicked(){ui.ctx().copy_text(device.code.clone());}
                        ui.hyperlink_to("Open GitHub verification page",&device.uri);
                        ui.label("Approve in your browser; Quark will finish sign-in automatically.");
                    });
                }
                ui.small("OAuth requires a GitHub OAuth App client ID with Device Flow enabled. Credentials are stored by GitHub CLI, not Quark.");
            }
            ui.add_space(3.0);
            ui.horizontal(|ui|{
                ui.label("Project owner:");ui.add(egui::TextEdit::singleline(&mut self.owner).desired_width(180.0));
                if ui.add_enabled(!self.busy,egui::Button::new("Load projects")).clicked(){self.selected_project=None;self.board=None;self.load_projects();}
                if ui.add_enabled(!self.busy,egui::Button::new("Refresh selected")).clicked(){self.refresh_board();}
                ui.separator();ui.label(format!("{} projects",self.projects.len()));
            });
        });
    }
    fn sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("projects_sidebar")
            .default_width(250.0)
            .min_width(200.0)
            .resizable(true)
            .show(ctx, |ui| {
                if ui
                    .selectable_label(
                        self.page == Page::AllProjects,
                        "◈  All Projects & organisations",
                    )
                    .clicked()
                {
                    self.page = Page::AllProjects;
                }
                ui.separator();
                ui.strong(format!("Owner: {}", self.owner));
                let previous_owner = self.owner.clone();
                egui::ComboBox::from_id_salt("owner_chooser")
                    .selected_text(&self.owner)
                    .show_ui(ui, |ui| {
                        if !self.current_user.is_empty() {
                            ui.selectable_value(
                                &mut self.owner,
                                self.current_user.clone(),
                                format!("Personal · {}", self.current_user),
                            );
                        }
                        for org in &self.orgs {
                            ui.selectable_value(&mut self.owner, org.clone(), org);
                        }
                    });
                if self.owner != previous_owner {
                    self.projects = self
                        .all_projects
                        .iter()
                        .filter(|p| p.owner.eq_ignore_ascii_case(&self.owner))
                        .map(|p| p.project.clone())
                        .collect();
                    self.selected_project = None;
                    self.selected_item = None;
                    self.board = None;
                }
                ui.heading("Projects");
                ui.horizontal(|ui| {
                    ui.label("New:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.project_title)
                            .hint_text("Project title")
                            .desired_width(125.0),
                    );
                    if ui
                        .add_enabled(
                            !self.busy && !self.project_title.is_empty(),
                            egui::Button::new("Create"),
                        )
                        .clicked()
                    {
                        self.submit_project();
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("sidebar_scroll")
                    .show(ui, |ui| {
                        let projects = self.projects.clone();
                        let mut selected = None;
                        for p in projects {
                            let active = Some(p.number) == self.selected_project;
                            if ui
                                .add_enabled(
                                    !self.busy,
                                    egui::Button::new(if active {
                                        format!("● #{}  {}", p.number, p.title)
                                    } else {
                                        format!("#{}  {}", p.number, p.title)
                                    }),
                                )
                                .clicked()
                            {
                                selected = Some(p.number);
                            }
                        }
                        if let Some(n) = selected {
                            self.page = Page::Projects;
                            self.load_board(n);
                        }
                    });
                ui.separator();
                if ui
                    .selectable_label(self.page == Page::Projects, "▤  Project board / table")
                    .clicked()
                {
                    self.page = Page::Projects;
                }
                if ui
                    .selectable_label(self.page == Page::Issues, "＋  New issue / draft")
                    .clicked()
                {
                    self.page = Page::Issues;
                }
                if ui
                    .selectable_label(self.page == Page::Import, "⇧  Import CSV")
                    .clicked()
                {
                    self.page = Page::Import;
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.small("Native Rust • GitHub OAuth / CLI accounts");
                    if let Some(b) = &self.board {
                        ui.small(format!(
                            "{} items · {} fields",
                            b.items.len(),
                            b.fields.len()
                        ));
                    }
                });
            });
    }
    fn all_projects_page(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("All Projects");
            if ui
                .add_enabled(!self.busy, egui::Button::new("Refresh all"))
                .clicked()
            {
                self.discover();
            }
        });
        ui.label(format!(
            "{} Projects across personal and organisation workspaces",
            self.all_projects.len()
        ));
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.project_scope, Scope::All, "All");
            ui.selectable_value(&mut self.project_scope, Scope::Personal, "Personal");
            ui.selectable_value(
                &mut self.project_scope,
                Scope::Organisations,
                "Organisations",
            );
            ui.separator();
            ui.label("Organisation / owner:");
            egui::ComboBox::from_id_salt("all_owner_filter")
                .selected_text(if self.project_owner_filter.is_empty() {
                    "Every owner"
                } else {
                    &self.project_owner_filter
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.project_owner_filter,
                        String::new(),
                        "Every owner",
                    );
                    ui.selectable_value(
                        &mut self.project_owner_filter,
                        self.current_user.clone(),
                        format!("Personal · {}", self.current_user),
                    );
                    for org in &self.orgs {
                        ui.selectable_value(&mut self.project_owner_filter, org.clone(), org);
                    }
                });
            ui.checkbox(&mut self.show_closed, "Include closed Projects");
        });
        ui.horizontal(|ui| {
            ui.label("Search:");
            ui.add(
                egui::TextEdit::singleline(&mut self.project_search)
                    .hint_text("Project title, owner, number")
                    .desired_width(340.0),
            );
        });
        for warning in &self.discovery_warnings {
            ui.colored_label(Color32::YELLOW, format!("Could not load: {warning}"));
        }
        let results = filters::filter_projects(
            &self.all_projects,
            &self.project_search,
            &self.project_owner_filter,
            &self.current_user,
            self.project_scope,
            self.show_closed,
        );
        ui.label(format!("{} matches", results.len()));
        let mut selected = None;
        egui::ScrollArea::both()
            .id_salt("all_projects_list")
            .show(ui, |ui| {
                egui::Grid::new("all_projects_grid")
                    .striped(true)
                    .min_col_width(100.0)
                    .show(ui, |ui| {
                        ui.strong("Owner / organisation");
                        ui.strong("Project");
                        ui.strong("Number");
                        ui.strong("State");
                        ui.end_row();
                        for item in results {
                            ui.label(&item.owner);
                            if ui.link(&item.project.title).clicked() {
                                selected = Some(item.clone());
                            }
                            ui.label(format!("#{}", item.project.number));
                            ui.label(if item.project.closed {
                                "Closed"
                            } else {
                                "Open"
                            });
                            ui.end_row();
                        }
                    });
            });
        if let Some(selected) = selected {
            self.open_owned_project(&selected);
        }
    }
    fn projects_page(&mut self, ui: &mut egui::Ui) {
        let Some(board) = self.board.clone() else {
            ui.heading("Choose a GitHub Project");
            ui.label("Select a Project on the left, or create a new one.");
            return;
        };
        ui.horizontal(|ui| {
            ui.heading(&board.project.title);
            ui.label(format!("#{}", board.project.number));
            if !board.project.url.is_empty() {
                ui.hyperlink_to("Open Project on GitHub", &board.project.url);
            }
            ui.separator();
            ui.selectable_value(&mut self.view, View::Table, "Table");
            ui.selectable_value(&mut self.view, View::Board, "Board");
            ui.separator();
            ui.label("Filter:");
            ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .desired_width(170.0)
                    .hint_text("Title, status, priority…"),
            );
        });
        ui.horizontal_wrapped(|ui| {
            for (name, selection) in [
                ("Status", &mut self.item_status),
                ("Priority", &mut self.item_priority),
                ("Size", &mut self.item_size),
            ] {
                let values = board
                    .fields
                    .iter()
                    .find(|f| f.name.eq_ignore_ascii_case(name))
                    .map(|f| f.options.iter().map(|o| o.name.clone()).collect::<Vec<_>>())
                    .unwrap_or_default();
                egui::ComboBox::from_id_salt(format!("filter_{name}"))
                    .selected_text(if selection.is_empty() {
                        format!("{name}: All")
                    } else {
                        format!("{name}: {selection}")
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(selection, String::new(), "All");
                        for value in values {
                            ui.selectable_value(selection, value.clone(), value);
                        }
                    });
            }
            if ui.button("Clear filters").clicked() {
                self.filter.clear();
                self.item_status.clear();
                self.item_priority.clear();
                self.item_size.clear();
            }
        });
        if let Some(warning) = &board.compatibility_warning {
            ui.colored_label(Color32::YELLOW, warning);
        }
        ui.separator();
        let items: Vec<Item> = filters::filter_items(
            &board.items,
            &self.filter,
            &self.item_status,
            &self.item_priority,
            &self.item_size,
        )
        .into_iter()
        .cloned()
        .collect();
        let mut clicked: Option<Item> = None;
        match self.view {
            View::Table => {
                egui::ScrollArea::both()
                    .id_salt("items_table")
                    .max_height(if self.selected_item.is_some() {
                        250.0
                    } else {
                        540.0
                    })
                    .show(ui, |ui| {
                        egui::Grid::new("item_grid")
                            .striped(true)
                            .min_col_width(90.0)
                            .show(ui, |ui| {
                                ui.strong("Issue / draft");
                                ui.strong("Status");
                                ui.strong("Priority");
                                ui.strong("Size");
                                ui.strong("Target version");
                                ui.end_row();
                                for item in items.iter().take(1500) {
                                    let active = Some(&item.id) == self.selected_item.as_ref();
                                    if ui.selectable_label(active, &item.title).clicked() {
                                        clicked = Some(item.clone());
                                    }
                                    for name in ["Status", "Priority", "Size", "Target Version"] {
                                        ui.label(get_value(item, name));
                                    }
                                    ui.end_row();
                                }
                            });
                    });
            }
            View::Board => {
                let statuses = board
                    .fields
                    .iter()
                    .find(|f| f.name.eq_ignore_ascii_case("Status"))
                    .map(|f| f.options.iter().map(|x| x.name.clone()).collect::<Vec<_>>())
                    .filter(|a| !a.is_empty())
                    .unwrap_or_else(|| {
                        vec![
                            "Backlog".into(),
                            "Ready".into(),
                            "In Progress".into(),
                            "Testing".into(),
                            "Done".into(),
                        ]
                    });
                let mut statuses = statuses;
                if !statuses.iter().any(|s| s == "Unassigned") {
                    statuses.push("Unassigned".into());
                }
                egui::ScrollArea::both()
                    .id_salt("board_scroll")
                    .max_height(480.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for status in &statuses {
                                ui.vertical(|ui| {
                                    ui.set_min_width(195.0);
                                    ui.set_max_width(220.0);
                                    ui.heading(status);
                                    let mut count = 0;
                                    for item in items.iter() {
                                        let value = get_value(item, "Status");
                                        if (value.is_empty() && status == "Unassigned")
                                            || value.eq_ignore_ascii_case(status)
                                        {
                                            count += 1;
                                            if ui
                                                .add(egui::Button::new(format!(
                                                    "{}\n{}",
                                                    item.title,
                                                    get_value(item, "Priority")
                                                )))
                                                .clicked()
                                            {
                                                clicked = Some(item.clone());
                                            }
                                        }
                                    }
                                    ui.small(format!("{count} items"));
                                });
                            }
                        });
                    });
            }
        }
        if let Some(item) = clicked {
            self.select_item(&item);
        }
        if items.len() > 1500 {
            ui.colored_label(
                Color32::YELLOW,
                "Showing first 1,500 items; narrow the filter to find the others.",
            );
        }
        if self.selected_item.is_some() {
            ui.separator();
            self.item_editor(ui, &board);
        }
        egui::CollapsingHeader::new("Manage Project settings and fields")
            .id_salt("project_settings")
            .show(ui, |ui| {
                ui.label("Project title");
                ui.add(
                    egui::TextEdit::singleline(&mut self.project_edit_title).desired_width(350.0),
                );
                ui.label("Project description");
                ui.add(
                    egui::TextEdit::singleline(&mut self.project_description).desired_width(500.0),
                );
                if ui
                    .add_enabled(!self.busy, egui::Button::new("Save Project settings"))
                    .clicked()
                {
                    self.save_project_settings();
                }
                ui.separator();
                ui.strong("Create custom field");
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.new_field_name)
                            .hint_text("Field name")
                            .desired_width(160.0),
                    );
                    egui::ComboBox::from_id_salt("new_field_kind")
                        .selected_text(&self.new_field_type)
                        .show_ui(ui, |ui| {
                            for kind in ["TEXT", "NUMBER", "DATE", "SINGLE_SELECT"] {
                                ui.selectable_value(
                                    &mut self.new_field_type,
                                    kind.to_owned(),
                                    kind,
                                );
                            }
                        });
                    if self.new_field_type == "SINGLE_SELECT" {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.new_field_options)
                                .hint_text("Option A,Option B")
                                .desired_width(210.0),
                        );
                    }
                    if ui
                        .add_enabled(
                            !self.busy && !self.new_field_name.trim().is_empty(),
                            egui::Button::new("Create field"),
                        )
                        .clicked()
                    {
                        self.add_custom_field();
                    }
                });
            });
    }
    fn item_editor(&mut self, ui: &mut egui::Ui, board: &Board) {
        let Some(item) = self.active_item() else {
            return;
        };
        ui.heading("Selected item");
        ui.horizontal_wrapped(|ui| {
            if !item.issue_type.is_empty() {
                ui.label(format!("Type: {}", item.issue_type));
            }
            if !item.state.is_empty() {
                ui.label(format!("State: {}", item.state));
            }
        });
        ui.add(
            egui::TextEdit::singleline(&mut self.edit_title)
                .desired_width(f32::INFINITY)
                .hint_text("Title"),
        );
        ui.add(
            egui::TextEdit::multiline(&mut self.edit_body)
                .desired_width(f32::INFINITY)
                .desired_rows(4)
                .hint_text("Issue description"),
        );
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!self.busy, egui::Button::new("Save title & description"))
                .clicked()
            {
                self.save_issue();
            }
            if !item.url.is_empty() {
                if ui
                    .add_enabled(!self.busy, egui::Button::new("Close issue"))
                    .clicked()
                {
                    self.close_selected(true);
                }
                if ui
                    .add_enabled(!self.busy, egui::Button::new("Reopen issue"))
                    .clicked()
                {
                    self.close_selected(false);
                }
                if ui.button("Copy issue URL").clicked() {
                    ui.ctx().copy_text(item.url.clone());
                }
            }
        });
        ui.separator();
        ui.label("Project custom fields");
        egui::Grid::new("field_values")
            .num_columns(3)
            .striped(true)
            .show(ui, |ui| {
                for field in &board.fields {
                    if field.name == "Title" {
                        continue;
                    }
                    ui.label(&field.name);
                    ui.label(get_value(&item, &field.name));
                    if ui.button("Edit").clicked() {
                        self.field_name = field.name.clone();
                        self.field_value = get_value(&item, &field.name);
                    }
                    ui.end_row();
                }
            });
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("field_select")
                .selected_text(if self.field_name.is_empty() {
                    "Select field"
                } else {
                    &self.field_name
                })
                .show_ui(ui, |ui| {
                    for f in &board.fields {
                        if f.name == "Title" {
                            continue;
                        }
                        if ui
                            .selectable_label(self.field_name == f.name, &f.name)
                            .clicked()
                        {
                            self.field_name = f.name.clone();
                            self.field_value = get_value(&item, &f.name);
                        }
                    }
                });
            let field = board.fields.iter().find(|f| f.name == self.field_name);
            if let Some(Field { options, .. }) = field {
                if !options.is_empty() {
                    egui::ComboBox::from_id_salt("field_option")
                        .selected_text(if self.field_value.is_empty() {
                            "(empty)"
                        } else {
                            &self.field_value
                        })
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(self.field_value.is_empty(), "(Clear)")
                                .clicked()
                            {
                                self.field_value.clear();
                            }
                            for option in options {
                                ui.selectable_value(
                                    &mut self.field_value,
                                    option.name.clone(),
                                    &option.name,
                                );
                            }
                        });
                } else {
                    ui.add(egui::TextEdit::singleline(&mut self.field_value).desired_width(180.0));
                }
                if ui
                    .add_enabled(!self.busy, egui::Button::new("Apply field"))
                    .clicked()
                {
                    self.set_selected_field();
                }
            }
        });
        ui.separator();
        ui.checkbox(
            &mut self.confirm_delete,
            "Confirm removal from this Project (repository issue is NOT deleted)",
        );
        if ui
            .add_enabled(
                self.confirm_delete && !self.busy,
                egui::Button::new(RichText::new("Remove from Project").color(Color32::LIGHT_RED)),
            )
            .clicked()
        {
            self.remove_selected();
            self.confirm_delete = false;
        }
    }
    fn create_page(&mut self, ui: &mut egui::Ui) {
        ui.heading("Create an issue");
        ui.label("Issues are created in a repository and linked to the selected Project. Drafts live only in the Project.");
        if let Some(b) = &self.board {
            ui.label(format!("Project: {}", b.project.title));
        } else {
            ui.colored_label(Color32::YELLOW, "Select a Project first");
        }
        ui.checkbox(
            &mut self.draft_issue,
            "Create Project draft instead of repository issue",
        );
        if !self.draft_issue {
            ui.horizontal(|ui| {
                ui.label("Repository:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.repo)
                        .desired_width(300.0)
                        .hint_text("owner/repository"),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Issue type:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.issue_type)
                        .desired_width(130.0)
                        .hint_text("Optional: Task / Bug"),
                );
                ui.label("Milestone:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.issue_milestone)
                        .desired_width(180.0)
                        .hint_text("Optional: existing milestone"),
                );
            });
        }
        ui.label("Title:");
        ui.add(egui::TextEdit::singleline(&mut self.issue_title).desired_width(f32::INFINITY));
        ui.label("Description (Markdown):");
        ui.add(
            egui::TextEdit::multiline(&mut self.issue_body)
                .desired_rows(10)
                .desired_width(f32::INFINITY),
        );
        if ui
            .add_enabled(
                !self.busy && self.board.is_some() && !self.issue_title.trim().is_empty(),
                egui::Button::new("Create and add to Project"),
            )
            .clicked()
        {
            self.submit_issue();
        }
    }
    fn import_page(&mut self, ui: &mut egui::Ui) {
        ui.heading("Import GitHub Issues from CSV");
        ui.label("Previews locally. When started, creates or reuses issues in the specified repositories and adds them to this Project.");
        ui.horizontal(|ui| {
            if ui.button("Choose CSV…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("CSV file", &["csv"])
                    .pick_file()
                {
                    self.load_csv(path);
                }
            }
            ui.label(&self.csv_path);
        });
        ui.horizontal(|ui| {
            ui.label("Default repository:");
            ui.add(egui::TextEdit::singleline(&mut self.repo).desired_width(300.0));
        });
        ui.checkbox(
            &mut self.create_fields,
            "Create missing Project fields (Priority, Size, Group, Target Version, etc.)",
        );
        ui.checkbox(
            &mut self.create_milestones,
            "Create missing GitHub issue milestones",
        );
        ui.label("Duplicates: exact-title matches in the same repository are reused; existing Project items are not re-added.");
        if let Some(board) = &self.board {
            ui.label(format!(
                "Destination: {} · Project #{}",
                board.project.title, board.project.number
            ));
        } else {
            ui.colored_label(
                Color32::YELLOW,
                "Choose a destination Project on the left first.",
            );
        }
        if !self.csv_rows.is_empty() {
            ui.heading(format!("Preview: {} rows", self.csv_rows.len()));
            let bad = self
                .csv_rows
                .iter()
                .filter(|r| {
                    !csv_import::valid_repository(if r.repository.is_empty() {
                        &self.repo
                    } else {
                        &r.repository
                    })
                })
                .count();
            if bad > 0 {
                ui.colored_label(
                    Color32::LIGHT_RED,
                    format!("{bad} rows have missing or invalid repositories"),
                );
            }
            egui::ScrollArea::vertical()
                .max_height(230.0)
                .show(ui, |ui| {
                    egui::Grid::new("preview").striped(true).show(ui, |ui| {
                        ui.strong("Title");
                        ui.strong("Priority");
                        ui.strong("Status");
                        ui.strong("Repository");
                        ui.end_row();
                        for row in self.csv_rows.iter().take(150) {
                            ui.label(&row.title);
                            ui.label(row.fields.get("Priority").map(String::as_str).unwrap_or(""));
                            ui.label(row.fields.get("Status").map(String::as_str).unwrap_or(""));
                            ui.label(if row.repository.is_empty() {
                                &self.repo
                            } else {
                                &row.repository
                            });
                            ui.end_row();
                        }
                    });
                });
            ui.add_space(8.0);
            if ui
                .add_enabled(
                    !self.busy && self.board.is_some() && bad == 0,
                    egui::Button::new(
                        RichText::new(format!(
                            "Import {} rows into selected Project",
                            self.csv_rows.len()
                        ))
                        .strong(),
                    ),
                )
                .clicked()
            {
                self.submit_csv();
            }
        }
    }
    fn activity_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("activity")
            .resizable(true)
            .default_height(110.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.strong("Activity log");
                    if ui.button("Clear").clicked() {
                        self.log.clear();
                    }
                    if !self.error.is_empty() {
                        ui.colored_label(Color32::LIGHT_RED, &self.error);
                    }
                });
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for line in self.log.iter().rev().take(180).rev() {
                            ui.small(line);
                        }
                    });
            });
    }
}
fn get_value(item: &Item, name: &str) -> String {
    item.fields
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.clone())
        .unwrap_or_default()
}
fn launch_login_terminal() -> Result<(), String> {
    const AUTH: &str = "gh auth login --web --git-protocol https --scopes project,repo,read:org";
    #[cfg(target_os = "windows")]
    let child = Command::new("cmd")
        .args(["/C", "start", "GitHub Sign-In", "cmd", "/K", AUTH])
        .spawn();
    #[cfg(target_os="macos")]
    let child=Command::new("osascript").args(["-e",&format!("tell application \"Terminal\" to do script \"export PATH=/opt/homebrew/bin:/usr/local/bin:$PATH; {AUTH}\"")]).spawn();
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let child = {
        let mut launched = None;
        for term in ["x-terminal-emulator", "gnome-terminal", "konsole", "xterm"] {
            let command =
                format!("{AUTH}; printf '\\nLogin finished. Press Enter to close. '; read _");
            let result = match term {
                "gnome-terminal" => Command::new(term)
                    .args(["--", "bash", "-lc", &command])
                    .spawn(),
                _ => Command::new(term)
                    .args(["-e", "bash", "-lc", &command])
                    .spawn(),
            };
            if result.is_ok() {
                launched = Some(result);
                break;
            }
        }
        launched.unwrap_or_else(|| {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No terminal emulator found",
            ))
        })
    };
    child.map(|_| ()).map_err(|e| {
        format!("Cannot start sign-in terminal: {e}. Run this command in a terminal: {AUTH}")
    })
}
impl eframe::App for DesktopApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.process_events();
        self.header(ctx);
        self.activity_panel(ctx);
        self.sidebar(ctx);
        egui::CentralPanel::default().show(ctx, |ui| match self.page {
            Page::AllProjects => self.all_projects_page(ui),
            Page::Projects => self.projects_page(ui),
            Page::Issues => self.create_page(ui),
            Page::Import => self.import_page(ui),
        });
        if self.busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(130));
        }
    }
}
fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Quark Projects")
            .with_inner_size([1200.0, 820.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Quark Projects",
        options,
        Box::new(|cc| Ok(Box::new(DesktopApp::new(cc)))),
    )
}

#[cfg(test)]
#[path = "tests/ui_extended.rs"]
mod ui_extended_tests;

#[cfg(test)]
#[path = "tests/stress_extended.rs"]
mod stress_extended_tests;
