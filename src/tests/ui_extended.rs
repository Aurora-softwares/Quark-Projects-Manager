use super::*;
use std::collections::BTreeMap;

fn sample_board() -> Board {
    Board {
        project: Project {
            number: 7,
            id: "PVT_7".into(),
            title: "Quark".into(),
            ..Default::default()
        },
        fields: vec![Field {
            id: "field".into(),
            name: "Status".into(),
            kind: "SINGLE_SELECT".into(),
            options: vec![github::OptionValue {
                name: "Ready".into(),
                id: "opt".into(),
            }],
        }],
        compatibility_warning: None,
        items: vec![
            Item {
                id: "I1".into(),
                title: "First".into(),
                body: "Body".into(),
                url: "https://github.com/org/repo/issues/1".into(),
                fields: BTreeMap::from([("Status".into(), "Ready".into())]),
                ..Default::default()
            },
            Item {
                id: "D2".into(),
                title: "Draft".into(),
                body: "Draft text".into(),
                ..Default::default()
            },
        ],
    }
}
fn render(app: &mut DesktopApp, page: Page) {
    app.page = page;
    let ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 900.0));
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| match app.page {
                Page::AllProjects => app.all_projects_page(ui),
                Page::Projects => app.projects_page(ui),
                Page::Issues => app.create_page(ui),
                Page::Import => app.import_page(ui),
            });
        },
    );
    assert!(!output.shapes.is_empty());
}
#[test]
fn test_state_is_disconnected_and_idle() {
    let app = DesktopApp::empty();
    assert!(!app.busy);
    assert!(app.projects.is_empty());
    assert!(app.board.is_none());
    assert!(app.selected_item.is_none());
}
#[test]
fn default_view_and_filters_are_clear() {
    let app = DesktopApp::empty();
    assert!(matches!(app.page, Page::AllProjects));
    assert!(matches!(app.view, View::Table));
    assert!(app.filter.is_empty() && app.item_priority.is_empty());
}
#[test]
fn cannot_submit_issue_without_project() {
    let mut app = DesktopApp::empty();
    app.issue_title = "New issue".into();
    app.submit_issue();
    assert!(app.error.contains("Choose a Project"));
    assert!(!app.busy);
}
#[test]
fn cannot_submit_csv_without_destination_project() {
    let mut app = DesktopApp::empty();
    app.submit_csv();
    assert!(app.error.contains("Select a project"));
}
#[test]
fn cannot_import_without_csv_even_with_project_loaded() {
    let mut app = DesktopApp::empty();
    app.board = Some(sample_board());
    app.submit_csv();
    assert!(app.error.contains("Select a CSV"));
    assert!(!app.busy);
}
#[test]
fn cannot_change_custom_field_without_selection() {
    let mut app = DesktopApp::empty();
    app.set_selected_field();
    assert!(!app.busy);
}
#[test]
fn choose_item_populates_editor_fields() {
    let mut app = DesktopApp::empty();
    let board = sample_board();
    app.board = Some(board.clone());
    app.select_item(&board.items[0]);
    assert_eq!(app.selected_item.as_deref(), Some("I1"));
    assert_eq!(app.edit_title, "First");
    assert_eq!(app.edit_body, "Body");
}
#[test]
fn choose_item_clears_previous_field_and_confirmation() {
    let mut app = DesktopApp::empty();
    let board = sample_board();
    app.field_name = "Priority".into();
    app.field_value = "P0".into();
    app.confirm_delete = true;
    app.select_item(&board.items[1]);
    assert!(app.field_name.is_empty() && app.field_value.is_empty());
    assert!(!app.confirm_delete);
}
#[test]
fn active_item_finds_correct_id_only() {
    let mut app = DesktopApp::empty();
    app.board = Some(sample_board());
    app.selected_item = Some("D2".into());
    assert_eq!(app.active_item().unwrap().title, "Draft");
    app.selected_item = Some("Nonexisting".into());
    assert!(app.active_item().is_none());
}
#[test]
fn active_item_requires_board() {
    let mut app = DesktopApp::empty();
    app.selected_item = Some("I1".into());
    assert!(app.active_item().is_none());
}
#[test]
fn field_value_lookup_is_case_insensitive() {
    let item = &sample_board().items[0];
    assert_eq!(get_value(item, "sTaTuS"), "Ready");
    assert_eq!(get_value(item, "Missing"), "");
}
#[test]
fn loading_csv_sets_rows_and_path() {
    let mut app = DesktopApp::empty();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("import.csv");
    std::fs::write(&path, "Title,Priority\nTest,P0\n").unwrap();
    app.load_csv(path.clone());
    assert_eq!(app.csv_rows.len(), 1);
    assert_eq!(app.csv_path, path.display().to_string());
}
#[test]
fn broken_csv_clears_old_import_preview() {
    let mut app = DesktopApp::empty();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("good.csv");
    std::fs::write(&path, "Title\nTest\n").unwrap();
    app.load_csv(path);
    assert_eq!(app.csv_rows.len(), 1);
    app.load_csv(dir.path().join("missing.csv"));
    assert!(app.csv_rows.is_empty());
    assert!(app.csv_path.is_empty());
    assert!(!app.error.is_empty());
}
#[test]
fn progress_events_append_to_log() {
    let mut app = DesktopApp::empty();
    app.tx
        .send(Event::Progress("Loaded organization".into()))
        .unwrap();
    app.process_events();
    assert_eq!(app.log, vec!["Loaded organization"]);
}
#[test]
fn excessive_activity_is_bounded() {
    let mut app = DesktopApp::empty();
    for n in 0..1200 {
        app.tx.send(Event::Progress(format!("{n}"))).unwrap();
    }
    app.process_events();
    assert!(app.log.len() <= 1000);
    assert!(app.log.iter().any(|s| s == "1199"));
}
#[test]
fn failed_async_operation_clears_busy_and_sets_error() {
    let mut app = DesktopApp::empty();
    app.busy = true;
    app.tx
        .send(Event::Complete(Err("API unavailable".into())))
        .unwrap();
    app.process_events();
    assert!(!app.busy);
    assert_eq!(app.error, "API unavailable");
}
#[test]
fn discovered_state_keeps_orgs_and_accounts() {
    let mut app = DesktopApp::empty();
    app.current_user = "robert".into();
    app.owner = "Aurora-softwares".into();
    app.tx
        .send(Event::Complete(Ok(Outcome::Discovered {
            accounts: vec![Account {
                login: "robert".into(),
                active: true,
                valid: true,
            }],
            orgs: vec!["Aurora-softwares".into()],
            entries: vec![OwnedProject {
                owner: "Aurora-softwares".into(),
                project: sample_board().project,
            }],
            warnings: vec!["Other org unreachable".into()],
        })))
        .unwrap();
    app.process_events();
    assert_eq!(app.projects.len(), 1);
    assert_eq!(app.account_choice, "robert");
    assert_eq!(app.orgs.len(), 1);
    assert_eq!(app.discovery_warnings.len(), 1);
}
#[test]
fn selecting_loaded_board_updates_project_edit_title() {
    let mut app = DesktopApp::empty();
    app.tx
        .send(Event::Complete(Ok(Outcome::Loaded(sample_board()))))
        .unwrap();
    app.process_events();
    assert_eq!(app.project_edit_title, "Quark");
    assert_eq!(app.board.unwrap().items.len(), 2);
}
#[test]
fn reloading_same_board_retains_selected_item() {
    let mut app = DesktopApp::empty();
    app.board = Some(sample_board());
    app.selected_item = Some("I1".into());
    app.tx
        .send(Event::Complete(Ok(Outcome::Loaded(sample_board()))))
        .unwrap();
    app.process_events();
    assert_eq!(app.selected_item.as_deref(), Some("I1"));
}
#[test]
fn reloading_board_removes_stale_selected_item() {
    let mut app = DesktopApp::empty();
    app.board = Some(sample_board());
    app.selected_item = Some("not-exists".into());
    app.tx
        .send(Event::Complete(Ok(Outcome::Loaded(sample_board()))))
        .unwrap();
    app.process_events();
    assert!(app.selected_item.is_none());
}
#[test]
fn empty_all_projects_view_renders_headlessly() {
    render(&mut DesktopApp::empty(), Page::AllProjects);
}
#[test]
fn populated_all_projects_view_renders_headlessly() {
    let mut app = DesktopApp::empty();
    app.current_user = "robert".into();
    app.orgs = vec!["Aurora-softwares".into()];
    app.all_projects = vec![OwnedProject {
        owner: "Aurora-softwares".into(),
        project: sample_board().project,
    }];
    render(&mut app, Page::AllProjects);
}
#[test]
fn empty_project_view_renders_headlessly() {
    render(&mut DesktopApp::empty(), Page::Projects);
}
#[test]
fn item_table_and_status_board_render_headlessly() {
    let mut app = DesktopApp::empty();
    app.board = Some(sample_board());
    app.view = View::Table;
    render(&mut app, Page::Projects);
    app.view = View::Board;
    render(&mut app, Page::Projects);
}
#[test]
fn issue_and_draft_editor_render_headlessly() {
    let mut app = DesktopApp::empty();
    app.board = Some(sample_board());
    app.draft_issue = false;
    render(&mut app, Page::Issues);
    app.draft_issue = true;
    render(&mut app, Page::Issues);
}
#[test]
fn csv_import_preview_renders_headlessly() {
    let mut app = DesktopApp::empty();
    app.board = Some(sample_board());
    app.csv_rows = vec![ImportRow {
        title: "A".into(),
        repository: "o/r".into(),
        fields: BTreeMap::from([("Priority".into(), "P0".into())]),
        ..Default::default()
    }];
    render(&mut app, Page::Import);
}
