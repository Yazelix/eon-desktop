use orbit_protocol::{
    Capabilities, Cell, CellStyle, CellWidth, Colors, Cursor, CursorShape, CursorViewport,
    Dimensions, Frame, Rgb, Row, Screen, StyleColor, Underline,
    session::{self, Failure, FailureCode, ServerMessage},
};
use yazelix_venus::{ConnectionState, LocalNoticeSource, ModelError, SessionModel};

#[test]
fn canonical_orbit_frame_becomes_one_deterministic_scene() {
    let mut model = SessionModel::new();
    apply_wire(
        &mut model,
        ServerMessage::Attached {
            version: session::VERSION,
        },
    )
    .unwrap();
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
fn attachment_and_server_failures_are_explicit_and_bounded() {
    let mut busy = SessionModel::new();
    busy.apply(ServerMessage::Busy).unwrap();
    busy.mark_lost("late socket close");
    assert_eq!(busy.connection(), &ConnectionState::Busy);
    assert!(busy.is_terminal());

    let mut incompatible = SessionModel::new();
    incompatible
        .apply(ServerMessage::Incompatible {
            minimum_version: 2,
            maximum_version: 3,
        })
        .unwrap();
    assert_eq!(
        incompatible.connection(),
        &ConnectionState::Incompatible {
            minimum: 2,
            maximum: 3
        }
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
fn copied_text_is_an_attached_one_shot_effect_not_presentation_state() {
    let mut model = SessionModel::new();
    assert_eq!(
        model
            .apply(ServerMessage::CopiedText("not attached".into()))
            .unwrap_err(),
        ModelError::UnexpectedMessage
    );

    let mut model = attached_model();
    model
        .apply(ServerMessage::Frame(Box::new(frame(1, Screen::Primary))))
        .unwrap();
    assert_eq!(
        model
            .apply(ServerMessage::CopiedText("e\u{301}界\nsecond".into()))
            .unwrap(),
        Some("e\u{301}界\nsecond".into())
    );
    assert_eq!(model.scene().unwrap().revision, 1);
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
    model
        .apply(ServerMessage::Attached {
            version: session::VERSION,
        })
        .unwrap();
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
