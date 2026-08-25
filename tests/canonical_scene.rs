use eon_workspace_protocol::{Pane, Snapshot, Tab};
use orbit_protocol::{
    Capabilities, Cell, CellStyle, CellWidth, Colors, Cursor, CursorShape, CursorViewport,
    Dimensions, Frame, Rgb, Row, Screen, StyleColor, Underline,
    session::{
        self, ClipboardLocation, Failure, FailureCode, PreviewOutcome, ScrollOutcome,
        ServerMessage, VerticalDirection, VerticalPreview, WheelOutcome,
    },
};
use winit::dpi::PhysicalSize;
use yazelix_venus::{
    CellMetrics, ClipboardEffect, ConnectionState, LocalNoticeSource, ModelError, ScenePreview,
    SessionModel, WorkspaceHit, WorkspaceScene,
};

#[test]
fn eon_workspace_becomes_one_bounded_native_accordion() {
    let snapshot = Snapshot {
        active_tab: "tab-1".into(),
        tabs: vec![
            Tab {
                id: "tab-1".into(),
                selected_pane: "pane-2".into(),
                panes: vec![
                    Pane {
                        id: "pane-1".into(),
                        session: "session-1".into(),
                        endpoint: b"/run/eon/orbit.sock".to_vec(),
                        live: true,
                    },
                    Pane {
                        id: "pane-2".into(),
                        session: "session-2".into(),
                        endpoint: b"/run/eon/session-2.sock".to_vec(),
                        live: true,
                    },
                ],
            },
            Tab {
                id: "tab-2".into(),
                selected_pane: "pane-3".into(),
                panes: vec![Pane {
                    id: "pane-3".into(),
                    session: "session-3".into(),
                    endpoint: b"/run/eon/session-3.sock".to_vec(),
                    live: false,
                }],
            },
        ],
    };
    let size = PhysicalSize::new(800, 600);
    let metrics = CellMetrics::for_scale(1.0);
    let initial = WorkspaceScene::from_snapshot(&snapshot, size, metrics, 0.0, 0.0);
    let scene = WorkspaceScene::from_snapshot(
        &snapshot,
        size,
        metrics,
        initial.active_tab_scroll(),
        initial.selected_pane_scroll(),
    );

    assert_eq!(
        scene
            .tabs
            .iter()
            .map(|tab| tab.id.as_str())
            .collect::<Vec<_>>(),
        ["tab-1", "tab-2"]
    );
    assert_eq!(
        scene
            .panes
            .iter()
            .map(|pane| pane.id.as_str())
            .collect::<Vec<_>>(),
        ["pane-1", "pane-2"]
    );
    assert_eq!(scene.panes.iter().filter(|pane| pane.selected).count(), 1);
    assert_eq!(scene.pane_scroll_limit(), 0.0);
    assert!(scene.panes.iter().all(|pane| {
        pane.rect.top >= scene.pane_viewport.top
            && pane.rect.bottom() <= scene.pane_viewport.bottom()
    }));
    assert_eq!(scene.panes[0].rect.bottom(), scene.panes[1].rect.top);
    assert!(scene.terminal.height > metrics.height);
    assert_eq!(scene.terminal.top, scene.panes[1].rect.bottom());
    assert_eq!(
        initial.hit_test(
            initial.panes[0].rect.left + 1.0,
            initial.panes[0].rect.top + 1.0
        ),
        Some(WorkspaceHit::Pane("pane-1"))
    );
    assert_eq!(
        scene.hit_test(scene.terminal.left + 1.0, scene.terminal.top + 1.0),
        Some(WorkspaceHit::Terminal)
    );
    assert!(scene.tab_scroll_limit().is_finite());
    assert!(scene.pane_scroll_limit().is_finite());

    let mut fitting_snapshot = snapshot.clone();
    fitting_snapshot.tabs[0].panes.push(Pane {
        id: "pane-4".into(),
        session: "session-4".into(),
        endpoint: b"/run/eon/session-4.sock".to_vec(),
        live: true,
    });
    let fitting = WorkspaceScene::from_snapshot(&fitting_snapshot, size, metrics, 0.0, 0.0);
    assert_eq!(fitting.pane_scroll_limit(), 0.0);
    assert!(fitting.panes.iter().all(|pane| {
        pane.rect.top >= fitting.pane_viewport.top
            && pane.rect.bottom() <= fitting.pane_viewport.bottom()
    }));
    assert_eq!(fitting.terminal.top, fitting.panes[1].rect.bottom());
    assert_eq!(fitting.panes[2].rect.top, fitting.terminal.bottom());

    let mut overflow_snapshot = fitting_snapshot;
    for number in 5..=32 {
        overflow_snapshot.tabs[0].panes.push(Pane {
            id: format!("pane-{number}"),
            session: format!("session-{number}"),
            endpoint: format!("/run/eon/session-{number}.sock").into_bytes(),
            live: true,
        });
    }
    overflow_snapshot.tabs[0].selected_pane = "pane-32".into();
    let unscrolled = WorkspaceScene::from_snapshot(&overflow_snapshot, size, metrics, 0.0, 0.0);
    assert!(unscrolled.pane_scroll_limit() > 0.0);
    let revealed = WorkspaceScene::from_snapshot(
        &overflow_snapshot,
        size,
        metrics,
        0.0,
        unscrolled.selected_pane_scroll(),
    );
    let selected = revealed
        .panes
        .iter()
        .find(|pane| pane.selected)
        .expect("selected pane");
    assert!(selected.rect.top >= revealed.pane_viewport.top);
    assert_eq!(revealed.terminal.top, selected.rect.bottom());
    assert!(revealed.terminal.bottom() <= revealed.pane_viewport.bottom());
    assert!(matches!(
        revealed.hit_test(1.0, revealed.tab_viewport.bottom() - 1.0),
        Some(WorkspaceHit::Tab(_))
    ));

    let mut second_tab = snapshot.clone();
    second_tab.active_tab = "tab-2".into();
    let narrow =
        WorkspaceScene::from_snapshot(&second_tab, PhysicalSize::new(100, 600), metrics, 0.0, 0.0);
    let revealed = WorkspaceScene::from_snapshot(
        &second_tab,
        PhysicalSize::new(100, 600),
        metrics,
        narrow.active_tab_scroll(),
        narrow.selected_pane_scroll(),
    );
    assert_eq!(
        revealed.hit_test(1.0, 1.0),
        Some(WorkspaceHit::Tab("tab-2"))
    );

    let tiny = WorkspaceScene::from_snapshot(
        &snapshot,
        PhysicalSize::new(1, 1),
        metrics,
        f32::NAN,
        f32::INFINITY,
    );
    assert_eq!((tiny.tab_scroll(), tiny.pane_scroll()), (0.0, 0.0));
    assert!(tiny.terminal.bottom() <= 1.0);
}

#[test]
fn canonical_session_revision_is_orbs_v10() {
    assert_eq!(session::VERSION, 10);
}

#[test]
fn canonical_orbit_frame_becomes_one_deterministic_scene() {
    let mut model = SessionModel::new();
    apply_wire(&mut model, ServerMessage::Attached).unwrap();
    apply_wire(
        &mut model,
        ServerMessage::Frame(Box::new(frame(7, Screen::Alternate))),
    )
    .unwrap();

    let scene = model.scene().unwrap();
    assert!(!scene.has_selected_content());
    assert_eq!(scene.content[0].cells[0].text, "e\u{301}");
    assert_eq!(scene.content[0].cells[1].text, "界");
    assert_eq!(scene.content[0].cells[1].width, CellWidth::Wide);
    assert_eq!(scene.content[0].cells[1].hyperlink, "https://yazelix.dev");
    assert_eq!(scene.accessible_text(), "e\u{301}界");
    assert_eq!(
        scene.snapshot(),
        concat!(
            "revision=7 screen=Alternate size=4x1 title=\"Eon ✦\" cwd=\"/tmp/venus\"\n",
            "run 0,0+1 \"e\\u{301}\" fg=f0f1f5 bg=10131a\n",
            "run 1,0+2 \"界\" fg=22cc88 bg=10131a\n",
            "cursor 1,0 Block visible=true tail=false\n"
        )
    );
}

#[test]
fn blinking_state_drives_native_wakeups_without_changing_frame_data() {
    let mut blinking = frame(8, Screen::Primary);
    blinking.rows[0].cells[0].style.blink = true;
    blinking.cursor.blinking = true;

    let scene = yazelix_venus::Scene::from_frame(&blinking);
    assert!(scene.has_blinking_content());
    assert!(scene.content[0].cells[0].style.blink);
    assert!(scene.cursor.unwrap().blinking);
}

#[test]
fn concealed_cells_stay_out_of_drawing_and_accessibility() {
    let mut concealed = frame(9, Screen::Primary);
    concealed.rows[0].cells[0].text = "secret".into();
    concealed.rows[0].cells[0].style.invisible = true;

    let scene = yazelix_venus::Scene::from_frame(&concealed);
    assert!(
        scene
            .glyph_runs()
            .iter()
            .all(|run| !run.text.contains("secret"))
    );
    assert_eq!(scene.accessible_text(), " 界");
}

#[test]
fn orbit_selected_style_is_the_only_visual_selection_source() {
    let mut selected = frame(10, Screen::Primary);
    selected.rows[0].cells[0].style.selected = true;

    let scene = yazelix_venus::Scene::from_frame(&selected);
    assert!(scene.has_selected_content());
    assert!(scene.content[0].cells[0].style.selected);
    assert_eq!(scene.content[0].cells[0].style.foreground, scene.background);
    assert_eq!(scene.content[0].cells[0].style.background, scene.foreground);
    assert_eq!(scene.accessible_text(), "e\u{301}界");
}

#[test]
fn ordered_frames_reject_stale_revisions_without_replacing_state() {
    let mut model = attached_model();
    model
        .apply(ServerMessage::Frame(Box::new(frame(9, Screen::Primary))))
        .unwrap();
    let error = model
        .apply(ServerMessage::Frame(Box::new(frame(9, Screen::Alternate))))
        .unwrap_err();
    assert!(matches!(error, ModelError::Frame(_)));
    assert_eq!(model.scene().unwrap().revision, 9);
    assert_eq!(model.scene().unwrap().screen, Screen::Primary);
}

#[test]
fn canonical_scroll_outcomes_reuse_the_existing_result_and_frame_owners() {
    let mut model = attached_model();
    model
        .apply(ServerMessage::Failure(Failure {
            code: FailureCode::InvalidInput,
            detail: "stale wheel".into(),
        }))
        .unwrap();
    model
        .apply(ServerMessage::WheelOutcome(WheelOutcome::TerminalRouted))
        .unwrap();
    assert!(model.notice().is_none());

    model
        .apply(ServerMessage::WheelOutcome(WheelOutcome::Viewport {
            applied_rows: -1,
            frame: Box::new(frame(1, Screen::Primary)),
        }))
        .unwrap();
    assert_eq!(model.scene().unwrap().revision, 1);

    model
        .apply(ServerMessage::VerticalPreview(VerticalPreview {
            frame_revision: 1,
            direction: VerticalDirection::Up,
            outcome: PreviewOutcome::TerminalRouted,
        }))
        .unwrap();
    assert!(matches!(
        model.scroll_preview(),
        Some(ScenePreview::TerminalOwned {
            frame_revision: 1,
            direction: VerticalDirection::Up,
        })
    ));
    assert_eq!(model.scene().unwrap().revision, 1);

    model
        .apply(ServerMessage::Failure(Failure {
            code: FailureCode::InvalidInput,
            detail: "stale scroll".into(),
        }))
        .unwrap();
    assert!(model.scroll_preview().is_none());
    assert_eq!(model.scene().unwrap().revision, 1);
}

#[test]
fn a_new_attachment_replaces_state_without_a_compatibility_window() {
    let mut first = attached_model();
    first
        .apply(ServerMessage::Frame(Box::new(frame(42, Screen::Primary))))
        .unwrap();

    let mut reopened = attached_model();
    reopened
        .apply(ServerMessage::Frame(Box::new(frame(1, Screen::Alternate))))
        .unwrap();
    assert_eq!(reopened.scene().unwrap().revision, 1);
    assert_eq!(reopened.scene().unwrap().screen, Screen::Alternate);
}

#[test]
fn reconnect_preserves_the_last_scene_and_accepts_a_fresh_revision() {
    let mut model = attached_model();
    model
        .apply(ServerMessage::Frame(Box::new(frame(42, Screen::Primary))))
        .unwrap();
    model.mark_lost("Orbit closed the local session");

    model.prepare_reconnect();
    assert_eq!(model.connection(), &ConnectionState::Connecting);
    assert_eq!(model.scene().unwrap().revision, 42);
    assert!(model.awaiting_current_frame());

    model.mark_lost("The selected Eon pane is offline");
    assert_eq!(
        model.connection(),
        &ConnectionState::Lost {
            detail: "The selected Eon pane is offline".into()
        }
    );
    assert_eq!(model.scene().unwrap().revision, 42);
    model.prepare_reconnect();

    model.apply(ServerMessage::Attached).unwrap();
    assert!(model.awaiting_current_frame());
    model
        .apply(ServerMessage::Frame(Box::new(frame(1, Screen::Alternate))))
        .unwrap();
    assert!(!model.awaiting_current_frame());
    assert_eq!(model.scene().unwrap().revision, 1);
    assert_eq!(model.scene().unwrap().screen, Screen::Alternate);
}

#[test]
fn attachment_and_server_failures_are_explicit_and_bounded() {
    let mut busy = SessionModel::new();
    busy.apply(ServerMessage::Busy).unwrap();
    busy.mark_lost("late socket close");
    assert_eq!(busy.connection(), &ConnectionState::Busy);
    assert!(busy.is_terminal());

    let mut incompatible = SessionModel::new();
    incompatible.mark_incompatible(3);
    assert_eq!(
        incompatible.connection(),
        &ConnectionState::Incompatible { version: 3 }
    );
    assert!(incompatible.is_terminal());

    let mut attached = attached_model();
    assert!(!attached.is_terminal());
    for (code, expected) in [
        (FailureCode::InvalidInput, "Orbit rejected input: detail"),
        (FailureCode::Protocol, "Orbit protocol failure: detail"),
        (FailureCode::Terminal, "Orbit terminal failure: detail"),
    ] {
        attached
            .apply(ServerMessage::Failure(Failure {
                code,
                detail: "detail".into(),
            }))
            .unwrap();
        assert_eq!(attached.notice(), Some(expected));
    }
    attached
        .apply(ServerMessage::Failure(Failure {
            code: FailureCode::InvalidInput,
            detail: "x".repeat(2_000),
        }))
        .unwrap();
    let notice = attached.notice().unwrap().to_owned();
    assert!(notice.starts_with("Orbit rejected input: "));
    assert_eq!(notice.chars().count(), 1_025);
    attached
        .apply(ServerMessage::Frame(Box::new(frame(1, Screen::Primary))))
        .unwrap();
    assert_eq!(attached.notice(), Some(notice.as_str()));
    attached.apply(ServerMessage::Accepted).unwrap();
    assert!(attached.notice().is_none());
    attached.mark_lost("Orbit closed the local session");
    attached.mark_lost("late socket error");
    attached.set_venus_notice(LocalNoticeSource::Input, "late input error");
    assert!(matches!(
        attached.connection(),
        ConnectionState::Lost { detail } if detail == "Orbit closed the local session"
    ));
    assert!(attached.notice().is_none());
}

#[test]
fn notices_recover_independently_by_source() {
    let mut model = attached_model();
    model.set_venus_notice(LocalNoticeSource::Input, "Venus could not encode input");
    model.apply(ServerMessage::Accepted).unwrap();
    assert_eq!(model.notice(), Some("Venus could not encode input"));
    assert!(!model.clear_venus_notice(LocalNoticeSource::Resize));
    model.set_venus_notice(LocalNoticeSource::Resize, "Window is too large");
    model.set_venus_notice(LocalNoticeSource::Queue, "Venus input queue is full");
    assert_eq!(model.notice(), Some("Venus input queue is full"));
    assert!(model.clear_venus_notice(LocalNoticeSource::Queue));
    assert_eq!(model.notice(), Some("Window is too large"));
    assert!(model.clear_venus_notice(LocalNoticeSource::Resize));
    assert_eq!(model.notice(), Some("Venus could not encode input"));
    assert!(model.clear_venus_notice(LocalNoticeSource::Input));
    model
        .apply(ServerMessage::Failure(Failure {
            code: FailureCode::InvalidInput,
            detail: "bad key".into(),
        }))
        .unwrap();
    assert!(!model.clear_venus_notice(LocalNoticeSource::Input));
    assert_eq!(model.notice(), Some("Orbit rejected input: bad key"));
    model.set_venus_notice(LocalNoticeSource::Queue, "Venus input queue is full");
    assert_eq!(model.notice(), Some("Venus input queue is full"));
    model.apply(ServerMessage::Accepted).unwrap();
    assert_eq!(model.notice(), Some("Venus input queue is full"));
    assert!(model.clear_venus_notice(LocalNoticeSource::Queue));
    assert!(model.notice().is_none());
}

#[test]
fn clipboard_text_is_an_attached_one_shot_effect_not_presentation_state() {
    let mut model = SessionModel::new();
    assert_eq!(
        model
            .apply(ServerMessage::CopiedText {
                location: ClipboardLocation::Selection,
                text: "not attached".into(),
            })
            .unwrap_err(),
        ModelError::UnexpectedMessage
    );
    assert_eq!(
        model
            .apply(ServerMessage::ClipboardWrite {
                location: ClipboardLocation::Primary,
                text: "not attached".into(),
            })
            .unwrap_err(),
        ModelError::UnexpectedMessage
    );

    let mut model = attached_model();
    model
        .apply(ServerMessage::Frame(Box::new(frame(1, Screen::Primary))))
        .unwrap();
    assert_eq!(
        model
            .apply(ServerMessage::CopiedText {
                location: ClipboardLocation::Selection,
                text: "e\u{301}界\nsecond".into(),
            })
            .unwrap(),
        Some(ClipboardEffect::SelectionCopy {
            location: ClipboardLocation::Selection,
            text: "e\u{301}界\nsecond".into(),
        })
    );
    model
        .apply(ServerMessage::Failure(Failure {
            code: FailureCode::InvalidInput,
            detail: "bad key".into(),
        }))
        .unwrap();
    assert_eq!(
        model
            .apply(ServerMessage::ClipboardWrite {
                location: ClipboardLocation::Selection,
                text: "terminal".into(),
            })
            .unwrap(),
        Some(ClipboardEffect::TerminalWrite {
            location: ClipboardLocation::Selection,
            text: "terminal".into(),
        })
    );
    assert_eq!(model.notice(), Some("Orbit rejected input: bad key"));
    assert_eq!(model.apply(ServerMessage::Accepted).unwrap(), None);
    assert_eq!(
        model
            .apply(ServerMessage::SelectionFinished { frame_revision: 9 })
            .unwrap(),
        None
    );
    assert_eq!(model.scene().unwrap().revision, 1);
}

#[test]
fn revision_bound_scroll_previews_follow_atomic_frames() {
    let mut model = attached_model();
    let mut initial = frame(1, Screen::Primary);
    let row = initial.rows[0].clone();
    let mut older = row.clone();
    older.cells[0].text = "older".into();
    initial.dimensions.rows = 2;
    initial.rows.push(row.clone());
    model
        .apply(ServerMessage::Frame(Box::new(initial)))
        .unwrap();
    model
        .apply(ServerMessage::VerticalPreview(VerticalPreview {
            frame_revision: 1,
            direction: VerticalDirection::Up,
            outcome: PreviewOutcome::Viewport {
                cols: 4,
                edge_reached: false,
                rows: vec![row.clone(), older],
            },
        }))
        .unwrap();
    assert!(matches!(
        model.scroll_preview(),
        Some(ScenePreview::Viewport {
            frame_revision: 1,
            direction: VerticalDirection::Up,
            rows,
            ..
        }) if rows.len() == 2
            && rows[0].cells[0].text == "e\u{301}"
            && rows[1].cells[0].text == "older"
    ));

    let mut next_frame = frame(2, Screen::Primary);
    next_frame.dimensions.rows = 2;
    next_frame.rows.push(row.clone());
    for (cols, rows) in [(3, vec![row.clone()]), (4, vec![row.clone(); 3])] {
        assert_eq!(
            model
                .apply(ServerMessage::ScrollOutcome(ScrollOutcome::Viewport {
                    requested_rows: -1,
                    applied_rows: -1,
                    frame: Box::new(next_frame.clone()),
                    next: PreviewOutcome::Viewport {
                        cols,
                        edge_reached: false,
                        rows,
                    },
                }))
                .unwrap_err(),
            ModelError::UnexpectedMessage
        );
        assert_eq!(model.scene().unwrap().revision, 1);
        assert!(matches!(
            model.scroll_preview(),
            Some(ScenePreview::Viewport {
                frame_revision: 1,
                ..
            })
        ));
    }

    model
        .apply(ServerMessage::ScrollOutcome(ScrollOutcome::Viewport {
            requested_rows: -1,
            applied_rows: -1,
            frame: Box::new(next_frame),
            next: PreviewOutcome::Viewport {
                cols: 4,
                edge_reached: false,
                rows: vec![row],
            },
        }))
        .unwrap();
    assert_eq!(model.scene().unwrap().revision, 2);
    assert!(matches!(
        model.scroll_preview(),
        Some(ScenePreview::Viewport {
            frame_revision: 2,
            direction: VerticalDirection::Up,
            rows,
            ..
        }) if rows.len() == 1
    ));

    model
        .apply(ServerMessage::VerticalPreview(VerticalPreview {
            frame_revision: 1,
            direction: VerticalDirection::Up,
            outcome: PreviewOutcome::TerminalRouted,
        }))
        .unwrap();
    assert!(model.scroll_preview().is_none());
    assert_eq!(model.scene().unwrap().revision, 2);
}

#[test]
fn invalid_session_and_frame_bytes_never_reach_draw_state() {
    let mut encoded =
        session::encode_server_message(&ServerMessage::Frame(Box::new(frame(1, Screen::Primary))))
            .unwrap();
    encoded[0] ^= 1;
    assert!(session::decode_server_message(&encoded).is_err());

    let mut model = SessionModel::new();
    assert_eq!(
        model
            .apply(ServerMessage::Frame(Box::new(frame(1, Screen::Primary))))
            .unwrap_err(),
        ModelError::UnexpectedMessage
    );
    assert!(model.scene().is_none());

    assert_eq!(
        model.apply(ServerMessage::Accepted).unwrap_err(),
        ModelError::UnexpectedMessage
    );
    let mut attached = attached_model();
    assert_eq!(
        attached.apply(ServerMessage::Busy).unwrap_err(),
        ModelError::UnexpectedMessage
    );
    assert!(attached.is_attached());
}

fn attached_model() -> SessionModel {
    let mut model = SessionModel::new();
    model.apply(ServerMessage::Attached).unwrap();
    model
}

fn apply_wire(
    model: &mut SessionModel,
    message: ServerMessage,
) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = session::encode_server_message(&message)?;
    model.apply(session::decode_server_message(&bytes)?)?;
    Ok(())
}

fn frame(revision: u64, screen: Screen) -> Frame {
    let background = Rgb {
        r: 0x10,
        g: 0x13,
        b: 0x1a,
    };
    let foreground = Rgb {
        r: 0xf0,
        g: 0xf1,
        b: 0xf5,
    };
    let mut palette = [Rgb::BLACK; 256];
    palette[2] = Rgb {
        r: 0x22,
        g: 0xcc,
        b: 0x88,
    };
    Frame {
        revision,
        dimensions: Dimensions { cols: 4, rows: 1 },
        screen,
        title: "Eon ✦".into(),
        working_directory: "/tmp/venus".into(),
        capabilities: Capabilities {
            hyperlinks: true,
            kitty_graphics: false,
        },
        colors: Colors {
            background,
            foreground,
            cursor: None,
            palette,
        },
        cursor: Cursor {
            visible: true,
            blinking: false,
            password_input: false,
            shape: CursorShape::Block,
            viewport: Some(CursorViewport {
                x: 1,
                y: 0,
                at_wide_tail: false,
            }),
        },
        rows: vec![Row {
            wrapped: false,
            wrap_continuation: false,
            kitty_virtual_placeholder: false,
            cells: vec![
                cell("e\u{301}", CellWidth::Narrow, StyleColor::None, ""),
                cell(
                    "界",
                    CellWidth::Wide,
                    StyleColor::Palette(2),
                    "https://yazelix.dev",
                ),
                cell("", CellWidth::SpacerTail, StyleColor::None, ""),
                cell("", CellWidth::Narrow, StyleColor::None, ""),
            ],
        }],
    }
}

fn cell(text: &str, width: CellWidth, foreground: StyleColor, hyperlink: &str) -> Cell {
    Cell {
        width,
        style: CellStyle {
            foreground,
            background: StyleColor::None,
            underline_color: StyleColor::None,
            bold: false,
            italic: false,
            faint: false,
            blink: false,
            inverse: false,
            invisible: false,
            strikethrough: false,
            overline: false,
            selected: false,
            protected: false,
            underline: Underline::None,
        },
        text: text.into(),
        hyperlink: hyperlink.into(),
    }
}
