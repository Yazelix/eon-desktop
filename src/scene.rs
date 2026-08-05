use orbit_protocol::{
    Cell, CellStyle, CellWidth, CursorShape, Frame, Rgb, Screen, StyleColor, Underline,
};
use std::fmt::Write;

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

    /// Text exposed to native accessibility, derived directly from this scene.
    #[must_use]
    pub fn accessible_text(&self) -> String {
        let mut text = String::new();
        for (row_index, row) in self.content.iter().enumerate() {
            if row_index > 0 {
                text.push('\n');
            }
            let mut column = 0_usize;
            while column < row.cells.len() {
                let cell = &row.cells[column];
                let wide = cell.width == CellWidth::Wide;
                match cell.width {
                    CellWidth::Narrow | CellWidth::Wide => {
                        text.push_str(if cell.style.invisible || cell.text.is_empty() {
                            if wide { "  " } else { " " }
                        } else {
                            &cell.text
                        });
                    }
                    CellWidth::SpacerHead => text.push(' '),
                    CellWidth::SpacerTail => {}
                }
                column += usize::from(wide) + 1;
            }
            while text.ends_with(' ') {
                text.pop();
            }
        }
        text
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
