use crate::render::CellMetrics;
use eon_workspace_protocol::Snapshot;
use orbit_protocol::{
    Cell, CellStyle, CellWidth, CursorShape, Frame, Rgb, Screen, StyleColor, Underline,
};
use std::fmt::Write;
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

    fn contains(self, x: f32, y: f32) -> bool {
        (self.left..self.right()).contains(&x) && (self.top..self.bottom()).contains(&y)
    }

    pub(crate) fn intersection(self, other: Self) -> Option<Self> {
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
}

/// One Eon-authored pane header projected into native accordion geometry.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkspacePane {
    pub id: String,
    pub session: String,
    pub live: bool,
    pub selected: bool,
    pub rect: SceneRect,
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
}

impl WorkspaceScene {
    #[must_use]
    pub fn from_snapshot(
        snapshot: &Snapshot,
        size: PhysicalSize<u32>,
        metrics: CellMetrics,
        tab_scroll: f32,
        pane_scroll: f32,
    ) -> Self {
        let width = size.width as f32;
        let height = size.height as f32;
        let tab_height = (metrics.height * 1.75).round().max(1.0).min(height);
        let tab_width = (metrics.width * 14.0).round().max(72.0);
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
        let pane_height = (metrics.height * 1.5)
            .round()
            .max(1.0)
            .min(pane_viewport.height);
        let active_tab = snapshot
            .tabs
            .iter()
            .position(|tab| tab.id == snapshot.active_tab)
            .expect("EONW validates the active tab");
        let active = &snapshot.tabs[active_tab];
        let selected_pane = active
            .panes
            .iter()
            .position(|pane| pane.id == active.selected_pane)
            .expect("EONW validates the selected pane");
        let terminal_height = (pane_viewport.height - pane_height).max(0.0);
        let tab_scroll_limit = (snapshot.tabs.len() as f32 * tab_width - width).max(0.0);
        let pane_scroll_limit =
            ((active.panes.len().saturating_sub(1)) as f32 * pane_height).max(0.0);
        let tab_scroll = if tab_scroll.is_finite() {
            tab_scroll.clamp(0.0, tab_scroll_limit)
        } else {
            0.0
        };
        let pane_scroll = if pane_scroll.is_finite() {
            pane_scroll.clamp(0.0, pane_scroll_limit)
        } else {
            0.0
        };
        let tabs = snapshot
            .tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| WorkspaceTab {
                id: tab.id.clone(),
                selected: index == active_tab,
                rect: SceneRect {
                    left: index as f32 * tab_width - tab_scroll,
                    top: 0.0,
                    width: tab_width,
                    height: tab_height,
                },
            })
            .collect();
        let panes = active
            .panes
            .iter()
            .enumerate()
            .map(|(index, pane)| WorkspacePane {
                id: pane.id.clone(),
                session: pane.session.clone(),
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
            active_tab_scroll: (active_tab as f32 * tab_width - (width - tab_width).max(0.0) / 2.0)
                .clamp(0.0, tab_scroll_limit),
            selected_pane_scroll: (selected_pane as f32 * pane_height)
                .clamp(0.0, pane_scroll_limit),
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
    pub(crate) fn visible_terminal(&self) -> Option<SceneRect> {
        self.terminal.intersection(self.pane_viewport)
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
    pub protected: bool,
    pub underline: Underline,
}

impl DrawStyle {
    pub(crate) fn foreground_visible(self, blink_visible: bool) -> bool {
        !self.invisible && (blink_visible || !self.blink)
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

/// One deterministic row of draw cells.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrawRow {
    pub wrapped: bool,
    pub wrap_continuation: bool,
    pub kitty_virtual_placeholder: bool,
    pub cells: Vec<DrawCell>,
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

/// A contiguous same-style text run positioned in terminal cells.
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
    /// Materialize a validated Orbit frame without retaining a second wire schema.
    #[must_use]
    pub fn from_frame(frame: &Frame) -> Self {
        let background = Color::from(frame.colors.background);
        let foreground = Color::from(frame.colors.foreground);
        let content = frame
            .rows
            .iter()
            .map(|row| DrawRow {
                wrapped: row.wrapped,
                wrap_continuation: row.wrap_continuation,
                kitty_virtual_placeholder: row.kitty_virtual_placeholder,
                cells: row
                    .cells
                    .iter()
                    .map(|cell| DrawCell::from_protocol(cell, frame))
                    .collect(),
            })
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
        for (row_index, row) in self.content.iter().enumerate() {
            let mut current: Option<GlyphRun> = None;
            let mut column = 0_u16;
            while usize::from(column) < row.cells.len() {
                let cell = &row.cells[usize::from(column)];
                let span = match cell.width {
                    CellWidth::Narrow => 1,
                    CellWidth::Wide => 2,
                    CellWidth::SpacerHead | CellWidth::SpacerTail => {
                        finish_run(&mut runs, &mut current);
                        column += 1;
                        continue;
                    }
                };
                let text = if cell.text.is_empty() {
                    if span == 2 { "  " } else { " " }
                } else {
                    &cell.text
                };
                if !cell.style.foreground_visible(true) {
                    finish_run(&mut runs, &mut current);
                } else if let Some(run) = &mut current
                    && run.style == cell.style
                    && run.column + run.columns == column
                {
                    run.text.push_str(text);
                    run.columns += span;
                } else {
                    finish_run(&mut runs, &mut current);
                    current = Some(GlyphRun {
                        column,
                        row: u16::try_from(row_index).expect("frame row count fits u16"),
                        columns: span,
                        text: text.to_owned(),
                        style: cell.style,
                    });
                }
                column = column.saturating_add(span);
            }
            finish_run(&mut runs, &mut current);
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
            let mut characters = Vec::with_capacity(row.cells.len());
            let mut column = 0_usize;
            while column < row.cells.len() {
                let cell = &row.cells[column];
                let wide = cell.width == CellWidth::Wide;
                match cell.width {
                    CellWidth::Narrow | CellWidth::Wide => {
                        if cell.style.invisible || cell.text.is_empty() {
                            characters.extend(
                                (0..if wide { 2 } else { 1 }).map(|_| (' ', cell.style.selected)),
                            );
                        } else {
                            characters.extend(
                                cell.text
                                    .chars()
                                    .map(|character| (character, cell.style.selected)),
                            );
                        }
                    }
                    CellWidth::SpacerHead => characters.push((' ', cell.style.selected)),
                    CellWidth::SpacerTail => {}
                }
                column += usize::from(wide) + 1;
            }
            while characters
                .last()
                .is_some_and(|(character, selected)| *character == ' ' && !selected)
            {
                characters.pop();
            }

            let mut value = String::new();
            let mut character_lengths = Vec::with_capacity(characters.len() + 1);
            for (character_index, (character, selected)) in characters.into_iter().enumerate() {
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
                value.push(character);
                character_lengths.push(character.len_utf8() as u8);
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

fn finish_run(runs: &mut Vec<GlyphRun>, current: &mut Option<GlyphRun>) {
    if let Some(run) = current.take()
        && !run.text.trim_matches(' ').is_empty()
    {
        runs.push(run);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accessibility_projection_keeps_authoritative_selected_cells() {
        let style = |selected| DrawStyle {
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
            protected: false,
            underline: Underline::None,
        };
        let cell = |text: &str, width, selected| DrawCell {
            width,
            text: text.into(),
            hyperlink: String::new(),
            style: style(selected),
        };
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
                        cell("a", CellWidth::Narrow, false),
                        cell("e\u{301}", CellWidth::Narrow, true),
                        cell("", CellWidth::Narrow, true),
                        cell("", CellWidth::Narrow, false),
                    ],
                },
                DrawRow {
                    wrapped: false,
                    wrap_continuation: false,
                    kitty_virtual_placeholder: false,
                    cells: vec![
                        cell("界", CellWidth::Wide, true),
                        cell("", CellWidth::SpacerTail, true),
                        cell("", CellWidth::Narrow, false),
                        cell("", CellWidth::Narrow, false),
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
        assert_eq!(content.rows[0].character_lengths, [1, 1, 2, 1, 1]);
    }
}
