use crate::{model::directory_picker_visible, render::CellMetrics};
use eon_workspace_protocol::v4::Snapshot;
use orbit_protocol::{
    Cell, CellStyle, CellWidth, CursorShape, Frame, Rgb, Row, Screen, StyleColor, Underline,
    session::VerticalDirection,
};
use std::{ffi::OsStr, fmt::Write, os::unix::ffi::OsStrExt, path::Path};
use winit::dpi::PhysicalSize;

/// Physical rectangle shared by workspace drawing, hit testing, and accessibility.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SceneRect {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl SceneRect {
    #[must_use]
    pub fn right(self) -> f32 {
        self.left + self.width
    }

    #[must_use]
    pub fn bottom(self) -> f32 {
        self.top + self.height
    }

    pub fn contains(self, x: f32, y: f32) -> bool {
        (self.left..self.right()).contains(&x) && (self.top..self.bottom()).contains(&y)
    }

    pub fn intersection(self, other: Self) -> Option<Self> {
        let left = self.left.max(other.left);
        let top = self.top.max(other.top);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        (left < right && top < bottom).then_some(Self {
            left,
            top,
            width: right - left,
            height: bottom - top,
        })
    }
}

/// One Eon-authored tab projected into native geometry.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkspaceTab {
    pub id: String,
    pub selected: bool,
    pub rect: SceneRect,
    label: String,
    accessible_label: String,
}

impl WorkspaceTab {
    pub(crate) fn label(&self) -> &str {
        &self.label
    }

    pub(crate) fn accessible_label(&self) -> &str {
        &self.accessible_label
    }
}

/// One Eon-authored pane header projected into native accordion geometry.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkspacePane {
    pub id: String,
    pub live: bool,
    pub selected: bool,
    pub rect: SceneRect,
    label: String,
}

impl WorkspacePane {
    pub(crate) fn label(&self) -> &str {
        &self.label
    }
}

/// Latest bounded read-only metadata state for one visible Eon pane endpoint.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum PaneMetadata {
    Connecting,
    Available {
        working_directory: String,
    },
    #[default]
    Unavailable,
}

const MAX_PANE_METADATA_FIELD_CHARS: usize = 80;
const HOME_MARKER: &str = "\u{f015}";

fn pane_label(id: &str, live: bool, metadata: &PaneMetadata) -> String {
    if !live {
        return format!("{id} offline");
    }
    let working_directory = match metadata {
        PaneMetadata::Connecting => return format!("{id} connecting"),
        PaneMetadata::Available { working_directory } => working_directory,
        PaneMetadata::Unavailable => return format!("{id} unavailable"),
    };
    if working_directory.trim().is_empty() {
        return id.to_owned();
    }
    let home = std::env::var_os("HOME");
    let working_directory = bounded_metadata_field(&compact_working_directory(
        working_directory,
        home.as_deref().map(Path::new),
    ));
    format!("{id}  {working_directory}")
}

fn local_working_directory(value: &str) -> &str {
    value
        .strip_prefix("file://")
        .and_then(|path| path.find('/').map(|slash| &path[slash..]))
        .unwrap_or(value)
}

fn compact_working_directory(value: &str, home: Option<&Path>) -> String {
    let value = local_working_directory(value);
    let path = Path::new(value);
    if let Some(home) = home.filter(|home| !home.as_os_str().is_empty()) {
        if path == home {
            return HOME_MARKER.to_owned();
        }
        if let Ok(relative) = path.strip_prefix(home) {
            return format!("~/{}", relative.display());
        }
    }
    value.to_owned()
}

fn bounded_metadata_field(value: &str) -> String {
    let value: String = value
        .chars()
        .map(|character| {
            if character.is_control() {
                '\u{fffd}'
            } else {
                character
            }
        })
        .collect();
    let length = value.chars().count();
    if length <= MAX_PANE_METADATA_FIELD_CHARS {
        return value;
    }
    let (prefix, tail) = if let Some(tail) = value.strip_prefix("~/") {
        ("~/…/", tail)
    } else if let Some(tail) = value.strip_prefix('/') {
        ("…/", tail)
    } else {
        ("…", value.as_str())
    };
    let tail_length = MAX_PANE_METADATA_FIELD_CHARS - prefix.chars().count();
    let start = tail
        .char_indices()
        .nth(tail.chars().count().saturating_sub(tail_length))
        .map_or(0, |(index, _)| index);
    let truncated_component = start > 0 && !tail[..start].ends_with('/');
    let tail = &tail[start..];
    let tail = if truncated_component {
        tail.split_once('/').map_or(tail, |(_, tail)| tail)
    } else {
        tail
    };
    format!("{prefix}{tail}")
}

fn tab_labels(id: &str, directory: &[u8], home: Option<&Path>) -> (String, String) {
    let path = Path::new(OsStr::from_bytes(directory));
    let leaf = if home.is_some_and(|home| path == home) {
        "~".into()
    } else if path == Path::new("/") {
        "/".into()
    } else {
        path.file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy()
            .into_owned()
    };
    let number = id.strip_prefix('t').unwrap_or(id);
    let clean = |text: &str| {
        text.chars()
            .map(|c| if c.is_control() { '\u{fffd}' } else { c })
            .collect::<String>()
    };
    let full_path = clean(&path.as_os_str().to_string_lossy());
    (
        format!("{number}  {}", clean(&leaf)),
        format!("{id}  {full_path}"),
    )
}

pub(crate) fn tab_max_width(metrics: CellMetrics, viewport_width: f32) -> f32 {
    (metrics.font_size * 17.5).min((viewport_width - metrics.padding * 2.0 / 3.0).max(1.0))
}

/// Native workspace target at one physical point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceHit<'a> {
    Tab(&'a str),
    Pane(&'a str),
    Terminal,
}

/// Native focus region inside the one-window workspace.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WorkspaceFocus {
    #[default]
    Terminal,
    Tabs,
    Panes,
}

/// Deterministic native projection of one complete accepted Eon snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkspaceScene {
    pub tabs: Vec<WorkspaceTab>,
    pub panes: Vec<WorkspacePane>,
    pub terminal: SceneRect,
    pub tab_viewport: SceneRect,
    pub pane_viewport: SceneRect,
    tab_scroll: f32,
    pane_scroll: f32,
    tab_scroll_limit: f32,
    pane_scroll_limit: f32,
    active_tab_scroll: f32,
    selected_pane_scroll: f32,
    directory_picker: bool,
}

fn header_heights(metrics: CellMetrics) -> (f32, f32) {
    (
        (metrics.height * 1.75).round().max(1.0),
        (metrics.height * 1.5).round().max(1.0),
    )
}

impl WorkspaceScene {
    /// Extra pixels around an initially requested terminal grid.
    #[must_use]
    pub fn initial_overhead(snapshot: &Snapshot, metrics: CellMetrics) -> (f32, f32) {
        let (tab_height, pane_height) = header_heights(metrics);
        if directory_picker_visible(snapshot) {
            (metrics.width * 2.0, tab_height + metrics.height * 2.0)
        } else {
            let tab = snapshot
                .tabs
                .iter()
                .find(|tab| tab.id == snapshot.active_tab)
                .expect("EONW validates the active tab");
            (0.0, tab_height + pane_height * tab.panes.len() as f32)
        }
    }
    #[must_use]
    pub fn from_snapshot(
        snapshot: &Snapshot,
        size: PhysicalSize<u32>,
        metrics: CellMetrics,
        tab_scroll: f32,
        pane_scroll: f32,
        tab_text: impl FnMut(&str, &str) -> (String, f32),
    ) -> Self {
        Self::from_snapshot_with_metadata(
            snapshot,
            size,
            metrics,
            tab_scroll,
            pane_scroll,
            |_| None,
            tab_text,
        )
    }

    #[must_use]
    pub fn from_snapshot_with_metadata<'a>(
        snapshot: &Snapshot,
        size: PhysicalSize<u32>,
        metrics: CellMetrics,
        tab_scroll: f32,
        pane_scroll: f32,
        metadata: impl Fn(&[u8]) -> Option<&'a PaneMetadata>,
        mut tab_text: impl FnMut(&str, &str) -> (String, f32),
    ) -> Self {
        let width = size.width as f32;
        let height = size.height as f32;
        let (tab_height, pane_height) = header_heights(metrics);
        let tab_height = tab_height.min(height);
        let gap = (metrics.padding / 3.0).min(tab_height / 4.0);
        let max_tab_width = tab_max_width(metrics, width);
        let tab_viewport = SceneRect {
            left: 0.0,
            top: 0.0,
            width,
            height: tab_height,
        };
        let pane_viewport = SceneRect {
            left: 0.0,
            top: tab_height,
            width,
            height: (height - tab_height).max(0.0),
        };
        let active_tab = snapshot
            .tabs
            .iter()
            .position(|tab| tab.id == snapshot.active_tab)
            .expect("EONW validates the active tab");
        let home = std::env::var_os("HOME");
        let mut tab_end = gap;
        let mut tabs: Vec<_> = snapshot
            .tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                let (label, accessible_label) =
                    tab_labels(&tab.id, &tab.directory, home.as_deref().map(Path::new));
                let (label, text_width) = tab_text(&tab.id, &label);
                let tab_width = (text_width.ceil() + metrics.padding * 2.0)
                    .clamp((metrics.font_size * 4.0).min(max_tab_width), max_tab_width);
                let left = tab_end;
                tab_end += tab_width + gap;
                WorkspaceTab {
                    id: tab.id.clone(),
                    selected: index == active_tab,
                    rect: SceneRect {
                        left,
                        top: gap,
                        width: tab_width,
                        height: (tab_height - gap * 2.0).max(0.0),
                    },
                    label,
                    accessible_label,
                }
            })
            .collect();
        let tab_scroll_limit = (tab_end - width).max(0.0);
        let active = tabs[active_tab].rect;
        let active_tab_scroll =
            (active.left - (width - active.width).max(0.0) / 2.0).clamp(0.0, tab_scroll_limit);
        let tab_scroll = if tab_scroll.is_finite() {
            tab_scroll.clamp(0.0, tab_scroll_limit)
        } else {
            0.0
        };
        for tab in &mut tabs {
            tab.rect.left -= tab_scroll;
        }
        if directory_picker_visible(snapshot) {
            let horizontal_inset =
                if pane_viewport.width >= metrics.padding * 2.0 + metrics.width * 3.0 {
                    metrics.width
                } else {
                    0.0
                };
            let vertical_inset =
                if pane_viewport.height >= metrics.padding * 2.0 + metrics.height * 3.0 {
                    metrics.height
                } else {
                    0.0
                };
            return Self {
                tabs,
                panes: Vec::new(),
                terminal: SceneRect {
                    left: horizontal_inset,
                    top: pane_viewport.top + vertical_inset,
                    width: (pane_viewport.width - horizontal_inset * 2.0).max(0.0),
                    height: (pane_viewport.height - vertical_inset * 2.0).max(0.0),
                },
                tab_viewport,
                pane_viewport,
                tab_scroll,
                pane_scroll: 0.0,
                tab_scroll_limit,
                pane_scroll_limit: 0.0,
                active_tab_scroll,
                selected_pane_scroll: 0.0,
                directory_picker: true,
            };
        }

        let active = &snapshot.tabs[active_tab];
        let pane_height = pane_height.min(pane_viewport.height);
        let selected = active
            .selected_pane
            .as_ref()
            .expect("EONW validates the selected pane");
        let selected_pane = active
            .panes
            .iter()
            .position(|pane| pane.id == *selected)
            .expect("EONW validates the selected pane");
        let maximum_terminal_height = (pane_viewport.height - pane_height).max(0.0);
        let minimum_terminal_height =
            (metrics.padding * 2.0 + metrics.height).min(maximum_terminal_height);
        let pane_headers_height = active.panes.len() as f32 * pane_height;
        let terminal_height = (pane_viewport.height - pane_headers_height)
            .clamp(minimum_terminal_height, maximum_terminal_height);
        let pane_scroll_limit =
            (pane_headers_height + terminal_height - pane_viewport.height).max(0.0);
        let pane_scroll = if pane_scroll.is_finite() {
            pane_scroll.clamp(0.0, pane_scroll_limit)
        } else {
            0.0
        };
        let panes = active
            .panes
            .iter()
            .enumerate()
            .map(|(index, pane)| {
                let label = metadata(&pane.endpoint).map_or_else(
                    || pane_label(&pane.id, pane.live, &PaneMetadata::Unavailable),
                    |metadata| pane_label(&pane.id, pane.live, metadata),
                );
                WorkspacePane {
                    id: pane.id.clone(),
                    live: pane.live,
                    selected: index == selected_pane,
                    rect: SceneRect {
                        left: 0.0,
                        top: tab_height
                            + index as f32 * pane_height
                            + if index > selected_pane {
                                terminal_height
                            } else {
                                0.0
                            }
                            - pane_scroll,
                        width,
                        height: pane_height,
                    },
                    label,
                }
            })
            .collect();
        let terminal = SceneRect {
            left: 0.0,
            top: tab_height + (selected_pane + 1) as f32 * pane_height - pane_scroll,
            width,
            height: terminal_height,
        };

        Self {
            tabs,
            panes,
            terminal,
            tab_viewport,
            pane_viewport,
            tab_scroll,
            pane_scroll,
            tab_scroll_limit,
            pane_scroll_limit,
            active_tab_scroll,
            selected_pane_scroll: (selected_pane as f32 * pane_height)
                .clamp(0.0, pane_scroll_limit),
            directory_picker: false,
        }
    }

    #[must_use]
    pub(crate) fn pane_bounds(&self, pane: &WorkspacePane) -> SceneRect {
        SceneRect {
            height: if pane.selected {
                self.terminal.bottom() - pane.rect.top
            } else {
                pane.rect.height
            },
            ..pane.rect
        }
    }

    #[must_use]
    pub fn hit_test(&self, x: f32, y: f32) -> Option<WorkspaceHit<'_>> {
        for tab in &self.tabs {
            if tab
                .rect
                .intersection(self.tab_viewport)
                .is_some_and(|rect| rect.contains(x, y))
            {
                return Some(WorkspaceHit::Tab(&tab.id));
            }
        }
        for pane in &self.panes {
            if pane
                .rect
                .intersection(self.pane_viewport)
                .is_some_and(|rect| rect.contains(x, y))
            {
                return Some(WorkspaceHit::Pane(&pane.id));
            }
        }
        self.visible_terminal()
            .filter(|rect| rect.contains(x, y))
            .map(|_| WorkspaceHit::Terminal)
    }

    #[must_use]
    pub fn visible_terminal(&self) -> Option<SceneRect> {
        self.terminal.intersection(self.pane_viewport)
    }

    #[must_use]
    pub fn directory_picker(&self) -> bool {
        self.directory_picker
    }

    #[must_use]
    pub fn active_tab_scroll(&self) -> f32 {
        self.active_tab_scroll
    }

    #[must_use]
    pub fn selected_pane_scroll(&self) -> f32 {
        self.selected_pane_scroll
    }

    #[must_use]
    pub fn tab_scroll_limit(&self) -> f32 {
        self.tab_scroll_limit
    }

    #[must_use]
    pub fn pane_scroll_limit(&self) -> f32 {
        self.pane_scroll_limit
    }

    #[must_use]
    pub fn tab_scroll(&self) -> f32 {
        self.tab_scroll
    }

    #[must_use]
    pub fn pane_scroll(&self) -> f32 {
        self.pane_scroll
    }
}

/// Renderer-ready color with no protocol-relative palette reference left.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl From<Rgb> for Color {
    fn from(value: Rgb) -> Self {
        Self {
            r: value.r,
            g: value.g,
            b: value.b,
        }
    }
}

/// Resolved draw style for one canonical Orbit cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawStyle {
    pub foreground: Color,
    pub background: Color,
    pub underline_color: Color,
    pub bold: bool,
    pub italic: bool,
    pub faint: bool,
    pub blink: bool,
    pub invisible: bool,
    pub strikethrough: bool,
    pub overline: bool,
    pub selected: bool,
    pub background_is_default: bool,
    pub protected: bool,
    pub underline: Underline,
}

impl DrawStyle {
    pub(crate) fn foreground_visible(self, blink_visible: bool) -> bool {
        !self.invisible && (blink_visible || !self.blink)
    }

    pub(crate) fn foreground_alpha(self) -> u8 {
        if self.faint { 150 } else { 255 }
    }
}

/// One exact grid cell after palette and inverse-color resolution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrawCell {
    pub width: CellWidth,
    pub text: String,
    pub hyperlink: String,
    pub style: DrawStyle,
}

impl DrawCell {
    pub(crate) fn is_full_block(&self) -> bool {
        self.width == CellWidth::Narrow && self.text == "█"
    }
}

/// One deterministic row of draw cells.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrawRow {
    pub wrapped: bool,
    pub wrap_continuation: bool,
    pub kitty_virtual_placeholder: bool,
    pub cells: Vec<DrawCell>,
}

/// A contiguous visible row span, borrowing Orbit's exact target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hyperlink<'a> {
    pub row: u16,
    pub column: u16,
    pub columns: u16,
    pub uri: &'a str,
}

impl Hyperlink<'_> {
    #[must_use]
    pub fn rect(self, origin: SceneRect, metrics: CellMetrics) -> SceneRect {
        SceneRect {
            left: origin.left + metrics.padding + f32::from(self.column) * metrics.width,
            top: origin.top + metrics.padding + f32::from(self.row) * metrics.height,
            width: f32::from(self.columns) * metrics.width,
            height: metrics.height,
        }
    }

    #[must_use]
    pub fn contains(self, row: u16, column: u16) -> bool {
        self.row == row && (self.column..self.column + self.columns).contains(&column)
    }
}

impl DrawRow {
    fn hyperlinks(&self, row: u16) -> impl Iterator<Item = Hyperlink<'_>> {
        let mut column = 0;
        let visible_uri = |cell: &DrawCell| {
            matches!(cell.width, CellWidth::Narrow | CellWidth::Wide)
                && !cell.style.invisible
                && !cell.hyperlink.is_empty()
        };
        std::iter::from_fn(move || {
            while column < self.cells.len() {
                let cell = &self.cells[column];
                if !visible_uri(cell) {
                    column += 1;
                    continue;
                }
                let start = column;
                let uri = cell.hyperlink.as_str();
                while let Some(cell) = self.cells.get(column) {
                    if !visible_uri(cell) || cell.hyperlink != uri {
                        break;
                    }
                    column += if cell.width == CellWidth::Wide { 2 } else { 1 };
                }
                return Some(Hyperlink {
                    row,
                    column: start as u16,
                    columns: (column - start) as u16,
                    uri,
                });
            }
            None
        })
    }

    pub(crate) fn from_protocol(row: &Row, frame: &Frame) -> Self {
        Self {
            wrapped: row.wrapped,
            wrap_continuation: row.wrap_continuation,
            kitty_virtual_placeholder: row.kitty_virtual_placeholder,
            cells: row
                .cells
                .iter()
                .map(|cell| DrawCell::from_protocol(cell, frame))
                .collect(),
        }
    }

    pub(crate) fn append_glyph_runs(&self, row: u16, runs: &mut Vec<GlyphRun>) {
        let mut column = 0_u16;
        while usize::from(column) < self.cells.len() {
            let cell = &self.cells[usize::from(column)];
            let span = match cell.width {
                CellWidth::Narrow => 1,
                CellWidth::Wide => 2,
                CellWidth::SpacerHead | CellWidth::SpacerTail => {
                    column += 1;
                    continue;
                }
            };
            if cell.style.foreground_visible(true) && !cell.text.trim_matches(' ').is_empty() {
                runs.push(GlyphRun {
                    column,
                    row,
                    columns: span,
                    text: cell.text.clone(),
                    style: cell.style,
                });
            }
            column = column.saturating_add(span);
        }
    }
}

/// One Orbit vertical-preview result for an exact accepted scene.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScenePreview {
    TerminalOwned {
        frame_revision: u64,
        direction: VerticalDirection,
    },
    Viewport {
        frame_revision: u64,
        direction: VerticalDirection,
        edge_reached: bool,
        rows: Vec<DrawRow>,
    },
}

/// Renderer-ready cursor state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawCursor {
    pub visible: bool,
    pub blinking: bool,
    pub password_input: bool,
    pub shape: CursorShape,
    pub column: u16,
    pub row: u16,
    pub at_wide_tail: bool,
    pub color: Color,
}

impl DrawCursor {
    /// Leading visual column occupied by the cursor.
    #[must_use]
    pub fn leading_column(self) -> u16 {
        self.column.saturating_sub(u16::from(self.at_wide_tail))
    }
}

/// One complete authoritative cell positioned on the terminal grid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlyphRun {
    pub column: u16,
    pub row: u16,
    pub columns: u16,
    pub text: String,
    pub style: DrawStyle,
}

/// One immutable, revision-tagged Venus draw scene.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scene {
    pub revision: u64,
    pub columns: u16,
    pub rows: u16,
    pub screen: Screen,
    pub title: String,
    pub working_directory: String,
    pub background: Color,
    pub foreground: Color,
    pub cursor: Option<DrawCursor>,
    pub content: Vec<DrawRow>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AccessibleText {
    pub rows: Vec<AccessibleRow>,
    pub selection: Option<AccessibleSelection>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AccessibleRow {
    pub value: String,
    pub character_lengths: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AccessibleSelection {
    pub anchor: AccessiblePosition,
    pub focus: AccessiblePosition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AccessiblePosition {
    pub row: usize,
    pub character_index: usize,
}

impl AccessibleText {
    pub fn plain_text(&self) -> String {
        self.rows.iter().map(|row| row.value.as_str()).collect()
    }
}

impl Scene {
    pub fn hyperlinks(&self) -> impl Iterator<Item = Hyperlink<'_>> {
        self.content
            .iter()
            .enumerate()
            .flat_map(|(row, content)| content.hyperlinks(row as u16))
    }

    #[must_use]
    pub fn hyperlink_at(&self, row: u16, column: u16) -> Option<Hyperlink<'_>> {
        self.content
            .get(usize::from(row))?
            .hyperlinks(row)
            .find(|link| link.contains(row, column))
    }

    /// Materialize a validated Orbit frame without retaining a second wire schema.
    #[must_use]
    pub fn from_frame(frame: &Frame) -> Self {
        let background = Color::from(frame.colors.background);
        let foreground = Color::from(frame.colors.foreground);
        let content = frame
            .rows
            .iter()
            .map(|row| DrawRow::from_protocol(row, frame))
            .collect();
        let cursor = frame.cursor.viewport.map(|cursor| DrawCursor {
            visible: frame.cursor.visible,
            blinking: frame.cursor.blinking,
            password_input: frame.cursor.password_input,
            shape: frame.cursor.shape,
            column: cursor.x,
            row: cursor.y,
            at_wide_tail: cursor.at_wide_tail,
            color: Color::from(frame.colors.cursor.unwrap_or(frame.colors.foreground)),
        });

        Self {
            revision: frame.revision,
            columns: frame.dimensions.cols,
            rows: frame.dimensions.rows,
            screen: frame.screen,
            title: frame.title.clone(),
            working_directory: frame.working_directory.clone(),
            background,
            foreground,
            cursor,
            content,
        }
    }

    /// Build positioned text runs without changing grapheme strings.
    #[must_use]
    pub fn glyph_runs(&self) -> Vec<GlyphRun> {
        let mut runs = Vec::new();
        for (index, row) in self.content.iter().enumerate() {
            row.append_glyph_runs(
                u16::try_from(index).expect("frame row count fits u16"),
                &mut runs,
            );
        }
        runs
    }

    /// Whether native presentation needs a bounded blink wakeup.
    #[must_use]
    pub fn has_blinking_content(&self) -> bool {
        self.cursor
            .is_some_and(|cursor| cursor.visible && cursor.blinking)
            || self
                .content
                .iter()
                .flat_map(|row| &row.cells)
                .any(|cell| cell.style.blink && cell.style.foreground_visible(true))
    }

    /// Whether the authoritative frame contains selected presentation.
    #[must_use]
    pub fn has_selected_content(&self) -> bool {
        self.content
            .iter()
            .flat_map(|row| &row.cells)
            .any(|cell| cell.style.selected)
    }

    /// Text exposed to native accessibility, derived directly from this scene.
    #[must_use]
    pub fn accessible_text(&self) -> String {
        self.accessible_content().plain_text()
    }

    pub(crate) fn accessible_content(&self) -> AccessibleText {
        let mut rows = Vec::with_capacity(self.content.len());
        let mut anchor = None;
        let mut focus = None;
        for (row_index, row) in self.content.iter().enumerate() {
            let mut units = Vec::with_capacity(row.cells.len());
            let mut column = 0_usize;
            while column < row.cells.len() {
                let cell = &row.cells[column];
                let wide = cell.width == CellWidth::Wide;
                match cell.width {
                    CellWidth::Narrow | CellWidth::Wide => {
                        if cell.style.invisible || cell.text.is_empty() {
                            units.extend(
                                (0..if wide { 2 } else { 1 })
                                    .map(|_| (" ", 1, cell.style.selected)),
                            );
                        } else {
                            let (text, length) = u8::try_from(cell.text.len())
                                .map_or(("\u{fffd}", 3), |length| (cell.text.as_str(), length));
                            units.push((text, length, cell.style.selected));
                        }
                    }
                    CellWidth::SpacerHead => units.push((" ", 1, cell.style.selected)),
                    CellWidth::SpacerTail => {}
                }
                column += usize::from(wide) + 1;
            }
            while units
                .last()
                .is_some_and(|(text, _, selected)| *text == " " && !selected)
            {
                units.pop();
            }

            let mut value = String::new();
            let mut character_lengths = Vec::with_capacity(units.len() + 1);
            for (character_index, (text, length, selected)) in units.into_iter().enumerate() {
                if selected {
                    anchor.get_or_insert(AccessiblePosition {
                        row: row_index,
                        character_index,
                    });
                    focus = Some(AccessiblePosition {
                        row: row_index,
                        character_index: character_index + 1,
                    });
                }
                value.push_str(text);
                character_lengths.push(length);
            }
            if row_index + 1 < self.content.len() {
                value.push('\n');
                character_lengths.push(1);
            }
            rows.push(AccessibleRow {
                value,
                character_lengths,
            });
        }
        AccessibleText {
            rows,
            selection: anchor
                .zip(focus)
                .map(|(anchor, focus)| AccessibleSelection { anchor, focus }),
        }
    }

    /// Stable human-readable state used by focused contract checks.
    #[must_use]
    pub fn snapshot(&self) -> String {
        let mut output = format!(
            "revision={} screen={:?} size={}x{} title={:?} cwd={:?}\n",
            self.revision, self.screen, self.columns, self.rows, self.title, self.working_directory
        );
        for run in self.glyph_runs() {
            let _ = writeln!(
                output,
                "run {},{}+{} {:?} fg={:02x}{:02x}{:02x} bg={:02x}{:02x}{:02x}",
                run.column,
                run.row,
                run.columns,
                run.text,
                run.style.foreground.r,
                run.style.foreground.g,
                run.style.foreground.b,
                run.style.background.r,
                run.style.background.g,
                run.style.background.b
            );
        }
        if let Some(cursor) = self.cursor {
            let _ = writeln!(
                output,
                "cursor {},{} {:?} visible={} tail={}",
                cursor.column, cursor.row, cursor.shape, cursor.visible, cursor.at_wide_tail
            );
        }
        output
    }
}

impl DrawCell {
    fn from_protocol(cell: &Cell, frame: &Frame) -> Self {
        let mut foreground = resolve_color(
            cell.style.foreground,
            frame.colors.foreground,
            &frame.colors.palette,
        );
        let mut background = resolve_color(
            cell.style.background,
            frame.colors.background,
            &frame.colors.palette,
        );
        if cell.style.inverse ^ cell.style.selected {
            std::mem::swap(&mut foreground, &mut background);
        }
        let underline_color = match cell.style.underline_color {
            StyleColor::None => foreground,
            color => resolve_color(color, frame.colors.foreground, &frame.colors.palette),
        };
        Self {
            width: cell.width,
            text: cell.text.clone(),
            hyperlink: cell.hyperlink.clone(),
            style: resolved_style(cell.style, foreground, background, underline_color),
        }
    }
}

fn is_default_background(style: CellStyle) -> bool {
    matches!(style.background, StyleColor::None) && !style.inverse && !style.selected
}

fn resolved_style(
    style: CellStyle,
    foreground: Color,
    background: Color,
    underline_color: Color,
) -> DrawStyle {
    DrawStyle {
        foreground,
        background,
        underline_color,
        bold: style.bold,
        italic: style.italic,
        faint: style.faint,
        blink: style.blink,
        invisible: style.invisible,
        strikethrough: style.strikethrough,
        overline: style.overline,
        selected: style.selected,
        background_is_default: is_default_background(style),
        protected: style.protected,
        underline: style.underline,
    }
}

fn resolve_color(color: StyleColor, default: Rgb, palette: &[Rgb; 256]) -> Color {
    Color::from(match color {
        StyleColor::None => default,
        StyleColor::Palette(index) => palette[usize::from(index)],
        StyleColor::Rgb(color) => color,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_directory_labels_keep_identity_and_bounded_path_context() {
        assert_eq!(
            tab_labels("t1", b"/home/alice", Some(Path::new("/home/alice"))),
            ("1  ~".into(), "t1  /home/alice".into())
        );
        assert_eq!(
            tab_labels("t2", b"/", Some(Path::new("/home/alice"))),
            ("2  /".into(), "t2  /".into())
        );
        assert_eq!(
            tab_labels("t3", b"/srv/nova", Some(Path::new("/home/alice"))),
            ("3  nova".into(), "t3  /srv/nova".into())
        );
        assert_eq!(
            tab_labels("t4", b"/tmp/eon-\xff", None),
            ("4  eon-�".into(), "t4  /tmp/eon-�".into())
        );
        let long = format!("/tmp/{}", "eon".repeat(100));
        let labels = tab_labels("t5", long.as_bytes(), None);
        assert_eq!(labels.0, format!("5  {}", "eon".repeat(100)));
        assert_eq!(labels.1, format!("t5  {long}"));
    }

    #[test]
    fn pane_metadata_label_is_bounded_safe_and_has_honest_fallbacks() {
        let metadata = PaneMetadata::Available {
            working_directory: format!("file:///tmp/{}", "eon".repeat(30)),
        };
        let label = pane_label("p1", true, &metadata);

        assert_eq!(label, format!("p1  …/{}", "eon".repeat(26)));
        assert!(!label.contains("file://"));
        assert_eq!(label.chars().count(), 84);
        assert_eq!(
            compact_working_directory("file:///tmp/eon", Some(Path::new(""))),
            "/tmp/eon"
        );
        let home = std::env::var("HOME").expect("HOME is required by Venus");
        assert_eq!(
            pane_label(
                "p1",
                true,
                &PaneMetadata::Available {
                    working_directory: format!("file://localhost{home}/pjs/yazelix-dir/eon"),
                },
            ),
            "p1  ~/pjs/yazelix-dir/eon"
        );
        let long_home_label = pane_label(
            "p1",
            true,
            &PaneMetadata::Available {
                working_directory: format!("{home}/{}/eon-desktop", "parent/".repeat(20)),
            },
        );
        assert!(long_home_label.starts_with("p1  ~/…/"));
        assert!(long_home_label.ends_with("/eon-desktop"));
        assert!(!long_home_label.contains(&home));
        assert!(long_home_label.chars().count() <= 84);
        assert_eq!(
            pane_label(
                "p1",
                true,
                &PaneMetadata::Available {
                    working_directory: "file://server/share".into(),
                },
            ),
            "p1  /share"
        );
        assert_eq!(
            pane_label(
                "p1",
                true,
                &PaneMetadata::Available {
                    working_directory: "file:///".into(),
                },
            ),
            "p1  /"
        );
        assert_eq!(
            pane_label(
                "p1",
                true,
                &PaneMetadata::Available {
                    working_directory: format!("file://host{home}"),
                },
            ),
            "p1  "
        );
        assert_eq!(
            pane_label(
                "p1",
                true,
                &PaneMetadata::Available {
                    working_directory: "file:///tmp/a\nb".into(),
                },
            ),
            "p1  /tmp/a�b"
        );
        assert_eq!(
            pane_label("p1", true, &PaneMetadata::Connecting),
            "p1 connecting"
        );
        assert_eq!(
            pane_label("p1", true, &PaneMetadata::Unavailable),
            "p1 unavailable"
        );
        assert_eq!(
            pane_label(
                "p1",
                true,
                &PaneMetadata::Available {
                    working_directory: String::new(),
                },
            ),
            "p1"
        );
        assert_eq!(pane_label("p1", false, &metadata), "p1 offline");
    }

    fn draw_style(selected: bool) -> DrawStyle {
        DrawStyle {
            foreground: Color::default(),
            background: Color::default(),
            underline_color: Color::default(),
            bold: false,
            italic: false,
            faint: false,
            blink: false,
            invisible: false,
            strikethrough: false,
            overline: false,
            selected,
            background_is_default: !selected,
            protected: false,
            underline: Underline::None,
        }
    }

    fn draw_cell(text: &str, width: CellWidth, selected: bool) -> DrawCell {
        DrawCell {
            width,
            text: text.into(),
            hyperlink: String::new(),
            style: draw_style(selected),
        }
    }

    #[test]
    fn background_opacity_provenance_distinguishes_default_explicit_selection_and_inverse() {
        let mut style = CellStyle {
            foreground: StyleColor::None,
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
        };

        assert!(is_default_background(style));
        style.background = StyleColor::Rgb(Rgb::BLACK);
        assert!(!is_default_background(style));
        style.background = StyleColor::None;
        style.selected = true;
        assert!(!is_default_background(style));
        style.selected = false;
        style.inverse = true;
        assert!(!is_default_background(style));
    }

    #[test]
    fn hyperlinks_follow_visible_heads_and_wide_cells() {
        let mut row = DrawRow {
            wrapped: false,
            wrap_continuation: false,
            kitty_virtual_placeholder: false,
            cells: vec![
                draw_cell("A", CellWidth::Narrow, false),
                draw_cell("界", CellWidth::Wide, false),
                draw_cell("", CellWidth::SpacerTail, false),
                draw_cell("hidden", CellWidth::Narrow, false),
                draw_cell("B", CellWidth::Narrow, false),
                draw_cell("", CellWidth::SpacerHead, false),
            ],
        };
        for cell in &mut row.cells {
            cell.hyperlink = "https://example.com/exact?x=%26&y=2".into();
        }
        row.cells[2].hyperlink = "https://wrong-tail.invalid".into();
        row.cells[3].style.invisible = true;
        let links = row.hyperlinks(2).collect::<Vec<_>>();
        assert_eq!(links.len(), 2);
        assert_eq!((links[0].row, links[0].column, links[0].columns), (2, 0, 3));
        assert_eq!(links[0].uri, row.cells[0].hyperlink);
        assert!(links[0].contains(2, 2));
        assert!(!links[0].contains(2, 3));
        assert_eq!((links[1].column, links[1].columns), (4, 1));
    }

    #[test]
    fn glyph_runs_preserve_authoritative_cell_starts() {
        let scene = Scene {
            revision: 1,
            columns: 10,
            rows: 1,
            screen: Screen::Primary,
            title: String::new(),
            working_directory: String::new(),
            background: Color::default(),
            foreground: Color::default(),
            cursor: None,
            content: vec![DrawRow {
                wrapped: false,
                wrap_continuation: false,
                kitty_virtual_placeholder: false,
                cells: vec![
                    draw_cell("A", CellWidth::Narrow, false),
                    draw_cell("e\u{301}", CellWidth::Narrow, false),
                    draw_cell("क्ष", CellWidth::Narrow, false),
                    draw_cell("界", CellWidth::Wide, false),
                    draw_cell("", CellWidth::SpacerTail, false),
                    draw_cell("👩‍💻", CellWidth::Wide, false),
                    draw_cell("", CellWidth::SpacerTail, false),
                    draw_cell("א", CellWidth::Narrow, false),
                    draw_cell("𐐀", CellWidth::Narrow, false),
                    draw_cell("│", CellWidth::Narrow, false),
                ],
            }],
        };

        let actual = scene
            .glyph_runs()
            .into_iter()
            .map(|run| (run.column, run.columns, run.text))
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            [
                (0, 1, "A"),
                (1, 1, "e\u{301}"),
                (2, 1, "क्ष"),
                (3, 2, "界"),
                (5, 2, "👩‍💻"),
                (7, 1, "א"),
                (8, 1, "𐐀"),
                (9, 1, "│"),
            ]
            .map(|(column, columns, text)| (column, columns, text.to_owned()))
        );
    }

    #[test]
    fn accessibility_projection_keeps_authoritative_selected_cells() {
        let scene = Scene {
            revision: 3,
            columns: 4,
            rows: 2,
            screen: Screen::Primary,
            title: String::new(),
            working_directory: String::new(),
            background: Color::default(),
            foreground: Color::default(),
            cursor: None,
            content: vec![
                DrawRow {
                    wrapped: false,
                    wrap_continuation: false,
                    kitty_virtual_placeholder: false,
                    cells: vec![
                        draw_cell("a", CellWidth::Narrow, false),
                        draw_cell("e\u{301}", CellWidth::Narrow, true),
                        draw_cell("", CellWidth::Narrow, true),
                        draw_cell("", CellWidth::Narrow, false),
                    ],
                },
                DrawRow {
                    wrapped: false,
                    wrap_continuation: false,
                    kitty_virtual_placeholder: false,
                    cells: vec![
                        draw_cell("界", CellWidth::Wide, true),
                        draw_cell("", CellWidth::SpacerTail, true),
                        draw_cell("", CellWidth::Narrow, false),
                        draw_cell("", CellWidth::Narrow, false),
                    ],
                },
            ],
        };

        let content = scene.accessible_content();
        assert_eq!(content.plain_text(), "ae\u{301} \n界");
        assert_eq!(
            content.selection,
            Some(AccessibleSelection {
                anchor: AccessiblePosition {
                    row: 0,
                    character_index: 1,
                },
                focus: AccessiblePosition {
                    row: 1,
                    character_index: 1,
                },
            })
        );
        assert_eq!(content.rows[0].character_lengths, [1, 3, 1, 1]);
    }

    #[test]
    fn accessibility_projection_keeps_cells_atomic_and_marks_oversized_text() {
        let scene = |cells: Vec<DrawCell>| Scene {
            revision: 4,
            columns: u16::try_from(cells.len()).unwrap(),
            rows: 1,
            screen: Screen::Primary,
            title: String::new(),
            working_directory: String::new(),
            background: Color::default(),
            foreground: Color::default(),
            cursor: None,
            content: vec![DrawRow {
                wrapped: false,
                wrap_continuation: false,
                kitty_virtual_placeholder: false,
                cells,
            }],
        };
        let selected_first_cell = Some(AccessibleSelection {
            anchor: AccessiblePosition {
                row: 0,
                character_index: 0,
            },
            focus: AccessiblePosition {
                row: 0,
                character_index: 1,
            },
        });

        let content = scene(vec![
            draw_cell("e\u{301}", CellWidth::Narrow, true),
            draw_cell("👩‍💻", CellWidth::Wide, false),
            draw_cell("", CellWidth::SpacerTail, false),
        ])
        .accessible_content();
        assert_eq!(content.rows[0].character_lengths, [3, 11]);
        assert_eq!(content.selection, selected_first_cell);

        let representable = format!("e{}", "\u{301}".repeat(127));
        let content =
            scene(vec![draw_cell(&representable, CellWidth::Narrow, true)]).accessible_content();
        assert_eq!(content.plain_text(), representable);
        assert_eq!(content.rows[0].character_lengths, [255]);

        let oversized = format!("e{}", "\u{301}".repeat(128));
        let scene = scene(vec![draw_cell(&oversized, CellWidth::Narrow, true)]);
        assert_eq!(scene.glyph_runs()[0].text, oversized);
        let content = scene.accessible_content();
        assert_eq!(content.plain_text(), "\u{fffd}");
        assert_eq!(content.rows[0].character_lengths, [3]);
        assert_eq!(content.selection, selected_first_cell);
    }
}
