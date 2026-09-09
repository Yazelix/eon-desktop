use crate::{
    Color as SceneColor, DrawCursor, DrawRow, DrawStyle, GlyphRun, Scene, ScenePreview, SceneRect,
    WorkspaceFocus, WorkspaceScene,
};
use glyphon::{
    Attrs, Buffer, Cache, Color, ColorMode, Family, FontSystem, Metrics, Resolution, Shaping,
    Style, SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport, Weight, Wrap,
};
use glyphon::{
    cosmic_text::{Fallback, PlatformFallback},
    fontdb,
};
use orbit_protocol::{CellWidth, CursorShape, Underline, session::VerticalDirection};
use std::{
    error::Error,
    fmt,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};
use unicode_script::Script;
use wgpu::{
    BlendState, BufferDescriptor, BufferUsages, ColorTargetState, ColorWrites,
    CommandEncoderDescriptor, CompositeAlphaMode, CurrentSurfaceTexture, DeviceDescriptor,
    DeviceLostReason, Error as WgpuError, FragmentState, Instance, InstanceDescriptor, LoadOp,
    MultisampleState, Operations, PipelineCompilationOptions, PipelineLayoutDescriptor,
    PresentMode, PrimitiveState, RenderPassColorAttachment, RenderPassDescriptor,
    RenderPipelineDescriptor, RequestAdapterOptions, ShaderModuleDescriptor, ShaderSource, StoreOp,
    Surface, SurfaceConfiguration, TextureViewDescriptor, VertexAttribute, VertexBufferLayout,
    VertexFormat, VertexState, VertexStepMode,
};
use winit::{dpi::PhysicalSize, event_loop::ActiveEventLoop, window::Window};

const VERTEX_SIZE: u64 = 24;
#[cfg(test)]
const VERTICES_PER_QUAD: u32 = 6;
const BRAILLE_FAMILY: &str = "DejaVu Sans";
const NERD_FONT_FAMILY: &str = "Symbols Nerd Font Mono";
const SHORT_CURSOR_ANIMATION: f32 = 0.04;
const LONG_CURSOR_ANIMATION: f32 = 0.15;
const MAX_CURSOR_DELTA: f32 = 0.1;
const CURSOR_SETTLED: f32 = 0.01;
const DEVICE_LOST: &str = "the Venus GPU device was lost";
const DEFAULT_METRICS: CellMetrics = CellMetrics {
    width: 10.0,
    height: 18.0,
    font_size: 16.0,
    padding: 12.0,
};
const DEFAULT_BACKGROUND: SceneColor = SceneColor {
    r: 10,
    g: 13,
    b: 20,
};
const SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@vertex
fn vertex(@location(0) position: vec2<f32>, @location(1) color: vec4<f32>) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(position, 0.0, 1.0);
    output.color = color;
    return output;
}

@fragment
fn fragment_linear(input: VertexOutput) -> @location(0) vec4<f32> {
    return input.color;
}

fn srgb_channel_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        return value / 12.92;
    }
    return pow((value + 0.055) / 1.055, 2.4);
}

@fragment
fn fragment_srgb(input: VertexOutput) -> @location(0) vec4<f32> {
    let rgb = vec3<f32>(
        srgb_channel_to_linear(input.color.r),
        srgb_channel_to_linear(input.color.g),
        srgb_channel_to_linear(input.color.b),
    );
    return vec4<f32>(rgb, input.color.a);
}
"#;

/// Physical cell layout used by rendering, input mapping, and resize requests.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellMetrics {
    pub width: f32,
    pub height: f32,
    pub font_size: f32,
    pub padding: f32,
}

/// Startup typography. Validation is shared by launch parsing and font resolution.
#[derive(Clone, Debug, PartialEq)]
pub struct FontSettings {
    pub family: Option<String>,
    pub fallbacks: Vec<String>,
    pub size: f32,
    pub line_height: f32,
}

impl Default for FontSettings {
    fn default() -> Self {
        Self {
            family: None,
            fallbacks: Vec::new(),
            size: DEFAULT_METRICS.font_size,
            line_height: DEFAULT_METRICS.height / DEFAULT_METRICS.font_size,
        }
    }
}

impl FontSettings {
    pub fn validate(&self) -> Result<(), RenderError> {
        if !self.size.is_finite()
            || !(6.0..=96.0).contains(&self.size)
            || !self.line_height.is_finite()
            || !(1.0..=3.0).contains(&self.line_height)
            || self.fallbacks.len() > 8
            || self.family.iter().chain(&self.fallbacks).any(|name| {
                name.is_empty()
                    || name.len() > 128
                    || name.trim() != name
                    || name.chars().any(char::is_control)
            })
        {
            return Err(RenderError("invalid terminal typography: font size must be 6..96, line height 1..3, with at most eight fallback families and nonempty trimmed family names up to 128 bytes without controls".into()));
        }
        Ok(())
    }
}

struct OrderedFallback {
    names: Vec<&'static str>,
    common: Vec<&'static str>,
    // Script is a u8 in the exact selected unicode-script release.
    scripts: [OnceLock<Vec<&'static str>>; 256],
}

impl Fallback for OrderedFallback {
    fn common_fallback(&self) -> &[&'static str] {
        &self.common
    }
    fn forbidden_fallback(&self) -> &[&'static str] {
        PlatformFallback.forbidden_fallback()
    }
    fn script_fallback(&self, script: Script, locale: &str) -> &[&'static str] {
        self.scripts[script as usize].get_or_init(|| {
            self.names
                .iter()
                .copied()
                .chain(
                    PlatformFallback
                        .script_fallback(script, locale)
                        .iter()
                        .copied(),
                )
                .collect()
        })
    }
}

/// Resolved fonts are admitted before native window creation and moved into its renderer.
pub struct FontSetup {
    font_system: FontSystem,
    logical: CellMetrics,
    family: Option<&'static str>,
    configured: bool,
}

impl FontSetup {
    pub fn new(settings: &FontSettings) -> Result<Self, RenderError> {
        Self::resolve(settings, FontSystem::new())
    }

    fn resolve(settings: &FontSettings, mut font_system: FontSystem) -> Result<Self, RenderError> {
        settings.validate()?;
        for (index, name) in settings
            .family
            .iter()
            .chain(&settings.fallbacks)
            .enumerate()
        {
            let id = font_system
                .db()
                .query(&fontdb::Query {
                    families: &[Family::Name(name)],
                    ..Default::default()
                })
                .ok_or_else(|| {
                    RenderError(format!("terminal font family is unavailable: {name}"))
                })?;
            if index == 0
                && settings.family.is_some()
                && !font_system.db().face(id).unwrap().monospaced
            {
                return Err(RenderError(format!(
                    "terminal primary font is not monospace: {name}"
                )));
            }
            if font_system.get_font(id, Weight::NORMAL).is_none() {
                return Err(RenderError(format!(
                    "cannot load terminal font family: {name}"
                )));
            }
        }
        let family = if settings.family.is_some() || !settings.fallbacks.is_empty() {
            let primary = if let Some(name) = &settings.family {
                name.clone()
            } else {
                let mut buffer =
                    Buffer::new(&mut font_system, Metrics::new(settings.size, settings.size));
                buffer.set_text(
                    " ",
                    &Attrs::new().family(Family::Monospace),
                    Shaping::Advanced,
                    None,
                );
                buffer.shape_until_scroll(&mut font_system, false);
                let id = buffer
                    .layout_runs()
                    .flat_map(|run| run.glyphs)
                    .next()
                    .ok_or_else(|| RenderError("no usable monospace font is installed".into()))?
                    .font_id;
                font_system.db().face(id).unwrap().families[0].0.clone()
            };
            // ponytail: cosmic-text requires static family names; at most nine bounded
            // startup names live for this process. Revisit with upstream runtime lists/live reload.
            let primary: &'static str = Box::leak(primary.into_boxed_str());
            let names: Vec<&'static str> = settings
                .fallbacks
                .iter()
                .map(|name| &*Box::leak(name.clone().into_boxed_str()))
                .collect();
            let common = names
                .iter()
                .copied()
                .chain(PlatformFallback.common_fallback().iter().copied())
                .collect();
            let (locale, db) = font_system.into_locale_and_db();
            font_system = FontSystem::new_with_locale_and_db_and_fallback(
                locale,
                db,
                OrderedFallback {
                    names,
                    common,
                    scripts: std::array::from_fn(|_| OnceLock::new()),
                },
            );
            Some(primary)
        } else {
            None
        };
        let width = if let Some(family) = family {
            let mut buffer =
                Buffer::new(&mut font_system, Metrics::new(settings.size, settings.size));
            buffer.set_text(
                " ",
                &Attrs::new().family(Family::Name(family)),
                Shaping::Advanced,
                None,
            );
            buffer
                .line_layout(&mut font_system, 0)
                .and_then(|lines| lines.first())
                .map(|line| line.w)
                .filter(|width| width.is_finite() && *width > 0.0)
                .ok_or_else(|| {
                    RenderError("terminal primary font has no usable cell advance".into())
                })?
        } else {
            DEFAULT_METRICS.width * settings.size / DEFAULT_METRICS.font_size
        };
        Ok(Self {
            font_system,
            logical: CellMetrics {
                width,
                height: settings.size * settings.line_height,
                font_size: settings.size,
                padding: DEFAULT_METRICS.padding,
            },
            family,
            configured: settings != &FontSettings::default(),
        })
    }

    #[must_use]
    pub fn metrics(&self, scale: f64) -> CellMetrics {
        self.logical.scaled(scale)
    }
}

#[derive(Clone, Copy)]
struct CellFont {
    family: Option<&'static str>,
    size: f32,
    top_offset: f32,
    letter_spacing: f32,
    box_size: f32,
    box_letter_spacing: f32,
}

impl CellFont {
    fn attrs(self) -> Attrs<'static> {
        Attrs::new()
            .family(self.family.map_or(Family::Monospace, Family::Name))
            .letter_spacing(self.letter_spacing)
    }
}

impl CellMetrics {
    #[must_use]
    pub fn for_scale(scale_factor: f64) -> Self {
        DEFAULT_METRICS.scaled(scale_factor)
    }

    fn scaled(self, scale_factor: f64) -> Self {
        let scale = scale_factor as f32;
        Self {
            width: (self.width * scale).round().max(1.0),
            height: (self.height * scale).round().max(1.0),
            font_size: (self.font_size * scale).max(1.0),
            padding: (self.padding * scale).round(),
        }
    }
}

/// Result of one bounded surface presentation attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentOutcome {
    Presented,
    Deferred,
    Occluded,
    Recovered,
}

/// GPU and text failure suitable for the client-visible bounded status path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderError(String);

impl fmt::Display for RenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for RenderError {}

struct PlacedText {
    buffer: Buffer,
    left: f32,
    top: f32,
    right: i32,
    bottom: i32,
    bound_left: i32,
    bound_top: i32,
    color: Color,
}

#[derive(Debug, PartialEq)]
struct ContentKey {
    generation: u64,
    scroll_offset: f32,
    workspace_focus: WorkspaceFocus,
    blink_visible: bool,
    preedit: String,
    status: String,
    hyperlink: Option<(u16, u16)>,
    hovered_header: Option<(WorkspaceFocus, String)>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct CursorPoint {
    x: f32,
    y: f32,
}

fn rect_corners(rect: SceneRect) -> [CursorPoint; 4] {
    [
        CursorPoint {
            x: rect.left,
            y: rect.top,
        },
        CursorPoint {
            x: rect.right(),
            y: rect.top,
        },
        CursorPoint {
            x: rect.right(),
            y: rect.bottom(),
        },
        CursorPoint {
            x: rect.left,
            y: rect.bottom(),
        },
    ]
}

#[derive(Clone, Copy, Debug, Default)]
struct CursorSpring {
    displacement: f32,
    velocity: f32,
}

impl CursorSpring {
    fn advance(&mut self, delta: f32, duration: f32) -> bool {
        let delta = delta.clamp(0.0, MAX_CURSOR_DELTA);
        if !self.displacement.is_finite()
            || !self.velocity.is_finite()
            || !duration.is_finite()
            || duration <= delta
        {
            *self = Self::default();
            return false;
        }
        if self.displacement.abs() < CURSOR_SETTLED {
            *self = Self::default();
            return false;
        }

        let omega = 4.0 / duration;
        let a = self.displacement;
        let b = a * omega + self.velocity;
        let decay = (-omega * delta).exp();
        self.displacement = (a + b * delta) * decay;
        self.velocity = decay * (-a * omega - b * delta * omega + b);
        if !self.displacement.is_finite() || self.displacement.abs() < CURSOR_SETTLED {
            *self = Self::default();
            false
        } else {
            true
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct AnimatedCorner {
    x: CursorSpring,
    y: CursorSpring,
    duration: f32,
}

#[derive(Clone, Debug, Default)]
struct CursorAnimation {
    corners: [AnimatedCorner; 4],
    target: Option<SceneRect>,
    route: Option<String>,
    viewport: Option<SceneRect>,
}

impl CursorAnimation {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn update(
        &mut self,
        target: SceneRect,
        route: Option<&str>,
        viewport: SceneRect,
        cell_width: f32,
        delta: f32,
        duration_scale: f32,
    ) -> bool {
        if !valid_rect(target)
            || !valid_rect(viewport)
            || !duration_scale.is_finite()
            || !(0.25..=4.0).contains(&duration_scale)
        {
            self.reset();
            return false;
        }
        if self.target.is_none()
            || self.route.as_deref() != route
            || self.viewport != Some(viewport)
        {
            self.snap(target, route, viewport);
            return false;
        }
        if self
            .target
            .is_some_and(|previous| rect_changed(previous, target))
        {
            self.start_jump(target, cell_width, duration_scale);
            return self.advance(0.0);
        }
        self.advance(delta)
    }

    fn snap(&mut self, target: SceneRect, route: Option<&str>, viewport: SceneRect) {
        self.target = Some(target);
        self.route = route.map(str::to_owned);
        self.viewport = Some(viewport);
        self.corners = [AnimatedCorner::default(); 4];
    }

    fn start_jump(&mut self, target: SceneRect, cell_width: f32, duration_scale: f32) {
        let previous = self.target.expect("an initialized cursor has a target");
        let movement = CursorPoint {
            x: target.left + target.width * 0.5 - previous.left - previous.width * 0.5,
            y: target.top + target.height * 0.5 - previous.top - previous.height * 0.5,
        };
        let short = movement.x.abs() <= cell_width * 2.001 && movement.y.abs() < CURSOR_SETTLED;
        let durations = if short {
            [SHORT_CURSOR_ANIMATION * duration_scale; 4]
        } else {
            ranked_cursor_durations(movement, duration_scale)
        };
        let previous = rect_corners(previous);
        let destinations = rect_corners(target);
        for (index, duration) in durations.into_iter().enumerate() {
            let corner = &mut self.corners[index];
            corner.x.displacement =
                destinations[index].x - (previous[index].x - corner.x.displacement);
            corner.y.displacement =
                destinations[index].y - (previous[index].y - corner.y.displacement);
            corner.duration = duration;
        }
        self.target = Some(target);
    }

    fn advance(&mut self, delta: f32) -> bool {
        let mut active = false;
        for corner in &mut self.corners {
            active |= corner.x.advance(delta, corner.duration);
            active |= corner.y.advance(delta, corner.duration);
        }
        active
    }

    fn corners(&self) -> [CursorPoint; 4] {
        let target = rect_corners(self.target.unwrap_or_default());
        [0, 1, 2, 3].map(|index| CursorPoint {
            x: target[index].x - self.corners[index].x.displacement,
            y: target[index].y - self.corners[index].y.displacement,
        })
    }

    fn is_active(&self) -> bool {
        self.corners
            .iter()
            .any(|corner| corner.x.displacement != 0.0 || corner.y.displacement != 0.0)
    }
}

fn ranked_cursor_durations(movement: CursorPoint, scale: f32) -> [f32; 4] {
    let length = (movement.x * movement.x + movement.y * movement.y)
        .sqrt()
        .max(f32::EPSILON);
    let direction = CursorPoint {
        x: movement.x / length,
        y: movement.y / length,
    };
    let relatives = [(-0.5_f32, -0.5_f32), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)];
    let ranked = relatives.map(|(x, y)| x * direction.x + y * direction.y);
    let mut order = [0_usize, 1, 2, 3];
    order.sort_by(|left, right| {
        ranked[*left]
            .total_cmp(&ranked[*right])
            .then(left.cmp(right))
    });
    let mut durations = [0.0; 4];
    for (rank, index) in order.into_iter().enumerate() {
        durations[index] = match rank {
            0 => LONG_CURSOR_ANIMATION * scale,
            1 => LONG_CURSOR_ANIMATION * scale * 0.5,
            _ => 0.0,
        };
    }
    durations
}

fn valid_rect(rect: SceneRect) -> bool {
    [rect.left, rect.top, rect.width, rect.height]
        .into_iter()
        .all(f32::is_finite)
        && rect.width > 0.0
        && rect.height > 0.0
}

fn shifted(mut rect: SceneRect, vertical: f32) -> SceneRect {
    rect.top += vertical;
    rect
}

fn scene_grid(scene: &Scene, viewport: SceneRect, metrics: CellMetrics) -> SceneRect {
    SceneRect {
        left: viewport.left + metrics.padding,
        top: viewport.top + metrics.padding,
        width: f32::from(scene.columns) * metrics.width,
        height: f32::from(scene.rows) * metrics.height,
    }
}

fn preview_rows(preview: Option<&ScenePreview>) -> Option<(VerticalDirection, &[DrawRow])> {
    let ScenePreview::Viewport {
        direction, rows, ..
    } = preview?
    else {
        return None;
    };
    Some((*direction, rows))
}

fn preview_row_top(
    scene: &Scene,
    direction: VerticalDirection,
    index: usize,
    metrics: CellMetrics,
    origin: SceneRect,
) -> f32 {
    match direction {
        VerticalDirection::Up => {
            origin.top + metrics.padding - (index as f32 + 1.0) * metrics.height
        }
        VerticalDirection::Down => {
            origin.top + metrics.padding + (f32::from(scene.rows) + index as f32) * metrics.height
        }
    }
}

fn row_intersects_clip(top: f32, height: f32, clip: SceneRect) -> bool {
    top < clip.bottom() && top + height > clip.top
}

fn rect_changed(left: SceneRect, right: SceneRect) -> bool {
    (left.left - right.left).abs() >= CURSOR_SETTLED
        || (left.top - right.top).abs() >= CURSOR_SETTLED
        || (left.width - right.width).abs() >= CURSOR_SETTLED
        || (left.height - right.height).abs() >= CURSOR_SETTLED
}

/// One wgpu surface and one glyphon text owner for the native window.
pub struct Renderer {
    instance: Instance,
    device: wgpu::Device,
    device_lost: Arc<AtomicBool>,
    queue: wgpu::Queue,
    surface: Surface<'static>,
    config: SurfaceConfiguration,
    rectangle_pipeline: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
    vertex_capacity: u64,
    vertex_count: u32,
    cursor_vertex_boundary: u32,
    dynamic_vertices: wgpu::Buffer,
    dynamic_vertex_capacity: u64,
    dynamic_vertex_count: u32,
    fonts: FontSetup,
    swash_cache: SwashCache,
    viewport: Viewport,
    atlas: TextAtlas,
    text_renderer: TextRenderer,
    tab_tooltip: Option<(usize, TextBounds)>,
    text: Vec<PlacedText>,
    content_key: Option<ContentKey>,
    hyperlink: Option<(u16, u16)>,
    hovered_header: Option<(WorkspaceFocus, String)>,
    cursor_tail: Option<(SceneColor, f32)>,
    cursor_animation: CursorAnimation,
    last_cursor_frame: Option<Instant>,
    clear: wgpu::Color,
    background_opacity: f32,
    pane_frames: bool,
    metrics: CellMetrics,
    cell_font: CellFont,
    window: Arc<Window>,
}

impl Renderer {
    pub fn set_hovered_header(&mut self, header: Option<(WorkspaceFocus, String)>) {
        self.hovered_header = header;
    }

    fn header_hovered(&self, focus: WorkspaceFocus, id: &str) -> bool {
        self.hovered_header
            .as_ref()
            .is_some_and(|(region, target)| *region == focus && target == id)
    }
    /// Fit the scene's canonical label using the same typography as its header.
    pub fn fit_tab_text(&mut self, label: &str) -> (String, f32) {
        let width = (crate::scene::tab_max_width(self.metrics, self.config.width as f32)
            - self.metrics.padding * 2.0)
            .max(0.0)
            .floor();
        fit_tab_text(&mut self.fonts.font_system, self.metrics, label, width)
    }

    pub fn set_hyperlink(&mut self, hyperlink: Option<(u16, u16)>) {
        self.hyperlink = hyperlink;
    }

    /// Notice placement shares the rendered link's geometry, keeping its target visible.
    #[must_use]
    pub fn notice_rect(&self, workspace: Option<&WorkspaceScene>) -> SceneRect {
        let height = self.metrics.height * if self.hyperlink.is_some() { 3.0 } else { 1.5 };
        let bottom = self.config.height as f32 - height - self.metrics.height * 0.5;
        let link_bottom = self.hyperlink.map(|(row, _)| {
            workspace.map_or(0.0, |workspace| workspace.terminal.top)
                + self.metrics.padding
                + f32::from(row + 1) * self.metrics.height
        });
        SceneRect {
            left: self.metrics.padding,
            top: if link_bottom.is_some_and(|link| link > bottom) {
                self.metrics.padding
            } else {
                bottom
            },
            width: self.config.width as f32 - self.metrics.padding * 2.0,
            height,
        }
    }

    pub async fn new(
        window: Arc<Window>,
        event_loop: &ActiveEventLoop,
        background_opacity: f32,
        cursor_tail: Option<(SceneColor, f32)>,
        pane_frames: bool,
        mut fonts: FontSetup,
    ) -> Result<Self, RenderError> {
        let size = nonzero(window.inner_size());
        let instance = Instance::new(InstanceDescriptor::new_with_display_handle(Box::new(
            event_loop.owned_display_handle(),
        )));
        let surface = instance
            .create_surface(Arc::clone(&window))
            .map_err(display_error("cannot create the Venus GPU surface"))?;
        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(display_error("no compatible Venus GPU adapter"))?;
        let (device, queue) = adapter
            .request_device(&DeviceDescriptor::default())
            .await
            .map_err(display_error("cannot create the Venus GPU device"))?;
        let device_lost = Arc::new(AtomicBool::new(false));
        let loss_state = Arc::clone(&device_lost);
        let redraw_window = Arc::clone(&window);
        device.set_device_lost_callback(move |reason, _| {
            if record_device_loss(&loss_state, reason) {
                redraw_window.request_redraw();
            }
        });
        let loss_state = Arc::clone(&device_lost);
        device.on_uncaptured_error(Arc::new(move |error| {
            handle_uncaptured_gpu_error(&loss_state, error);
        }));
        let mut config = surface
            .get_default_config(&adapter, size.width, size.height)
            .ok_or_else(|| RenderError("the GPU surface has no supported format".into()))?;
        let capabilities = surface.get_capabilities(&adapter);
        if let Some(format) = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
        {
            config.format = format;
        }
        let srgb_target = config.format.is_srgb();
        config.present_mode = PresentMode::Fifo;
        config.alpha_mode = surface_alpha_mode(background_opacity, &capabilities.alpha_modes)?;
        ensure_device_available(&device_lost)?;
        surface.configure(&device, &config);
        ensure_device_available(&device_lost)?;

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("venus rectangles"),
            source: ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("venus rectangle pipeline layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let rectangle_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("venus rectangle pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[Some(VertexBufferLayout {
                    array_stride: VERTEX_SIZE,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[
                        VertexAttribute {
                            format: VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        },
                        VertexAttribute {
                            format: VertexFormat::Float32x4,
                            offset: 8,
                            shader_location: 1,
                        },
                    ],
                })],
            },
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some(if srgb_target {
                    "fragment_srgb"
                } else {
                    "fragment_linear"
                }),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format: config.format,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let vertex_capacity = VERTEX_SIZE;
        let vertices = device.create_buffer(&BufferDescriptor {
            label: Some("venus rectangle vertices"),
            size: vertex_capacity,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let dynamic_vertices = device.create_buffer(&BufferDescriptor {
            label: Some("venus dynamic cursor vertices"),
            size: vertex_capacity,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let metrics = fonts.metrics(window.scale_factor());
        let cell_font = fitted_cell_font(
            &mut fonts.font_system,
            metrics,
            fonts.family,
            fonts.configured,
        );
        let swash_cache = SwashCache::new();
        let cache = Cache::new(&device);
        let viewport = Viewport::new(&device, &cache);
        let color_mode = if srgb_target {
            ColorMode::Accurate
        } else {
            ColorMode::Web
        };
        let mut atlas =
            TextAtlas::with_color_mode(&device, &queue, &cache, config.format, color_mode);
        let text_renderer =
            TextRenderer::new(&mut atlas, &device, MultisampleState::default(), None);
        ensure_device_available(&device_lost)?;
        Ok(Self {
            instance,
            device,
            device_lost,
            queue,
            surface,
            config,
            rectangle_pipeline,
            vertices,
            vertex_capacity,
            vertex_count: 0,
            cursor_vertex_boundary: 0,
            dynamic_vertices,
            dynamic_vertex_capacity: vertex_capacity,
            dynamic_vertex_count: 0,
            fonts,
            swash_cache,
            viewport,
            atlas,
            text_renderer,
            tab_tooltip: None,
            text: Vec::new(),
            content_key: None,
            hyperlink: None,
            hovered_header: None,
            cursor_tail,
            cursor_animation: CursorAnimation::default(),
            last_cursor_frame: None,
            clear: clear_color(DEFAULT_BACKGROUND, background_opacity, srgb_target),
            background_opacity,
            pane_frames,
            metrics,
            cell_font,
            window,
        })
    }

    #[must_use]
    pub fn metrics(&self) -> CellMetrics {
        self.metrics
    }

    #[must_use]
    pub fn metrics_at_scale(&self, scale: f64) -> CellMetrics {
        self.fonts.metrics(scale)
    }

    #[must_use]
    pub fn size(&self) -> PhysicalSize<u32> {
        PhysicalSize::new(self.config.width, self.config.height)
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>, scale_factor: f64) {
        let size = nonzero(size);
        self.config.width = size.width;
        self.config.height = size.height;
        let metrics = self.fonts.metrics(scale_factor);
        if metrics != self.metrics {
            self.cell_font = fitted_cell_font(
                &mut self.fonts.font_system,
                metrics,
                self.fonts.family,
                self.fonts.configured,
            );
            self.metrics = metrics;
        }
        self.surface.configure(&self.device, &self.config);
        self.content_key = None;
        self.reset_cursor_animation();
    }

    pub fn reset_cursor_animation(&mut self) {
        self.cursor_animation.reset();
        self.last_cursor_frame = None;
        self.dynamic_vertex_count = 0;
    }

    #[must_use]
    pub fn cursor_animation_active(&self) -> bool {
        self.cursor_animation.is_active()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        scene: Option<&Scene>,
        scroll_preview: Option<&ScenePreview>,
        scroll_offset: f32,
        workspace: Option<&WorkspaceScene>,
        workspace_focus: WorkspaceFocus,
        status: &str,
        blink_visible: bool,
        preedit: &str,
        generation: u64,
    ) -> Result<PresentOutcome, RenderError> {
        ensure_device_available(&self.device_lost)?;
        let content_changed = self.rebuild_if_needed(
            scene,
            scroll_preview,
            scroll_offset,
            workspace,
            workspace_focus,
            status,
            blink_visible,
            preedit,
            generation,
        );
        if scroll_offset == 0.0 {
            self.rebuild_dynamic_cursor(scene, workspace, blink_visible);
        } else {
            self.reset_cursor_animation();
        }
        if content_changed {
            self.viewport.update(
                &self.queue,
                Resolution {
                    width: self.config.width,
                    height: self.config.height,
                },
            );
            if self
                .text_renderer
                .prepare(
                    &self.device,
                    &self.queue,
                    &mut self.fonts.font_system,
                    &mut self.atlas,
                    &self.viewport,
                    text_areas(&self.text, self.tab_tooltip),
                    &mut self.swash_cache,
                )
                .is_err()
            {
                self.atlas.trim();
                self.text_renderer
                    .prepare(
                        &self.device,
                        &self.queue,
                        &mut self.fonts.font_system,
                        &mut self.atlas,
                        &self.viewport,
                        text_areas(&self.text, self.tab_tooltip),
                        &mut self.swash_cache,
                    )
                    .map_err(display_error("the Venus glyph atlas is full"))?;
            }
        }

        let current_texture = self.surface.get_current_texture();
        ensure_device_available(&self.device_lost)?;
        let frame = match current_texture {
            CurrentSurfaceTexture::Success(frame) => frame,
            CurrentSurfaceTexture::Timeout => {
                self.window.request_redraw();
                return Ok(PresentOutcome::Deferred);
            }
            CurrentSurfaceTexture::Occluded => {
                self.reset_cursor_animation();
                return Ok(PresentOutcome::Occluded);
            }
            CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                ensure_device_available(&self.device_lost)?;
                self.reset_cursor_animation();
                return Ok(PresentOutcome::Recovered);
            }
            CurrentSurfaceTexture::Suboptimal(frame) => {
                drop(frame);
                self.surface.configure(&self.device, &self.config);
                ensure_device_available(&self.device_lost)?;
                self.reset_cursor_animation();
                return Ok(PresentOutcome::Recovered);
            }
            CurrentSurfaceTexture::Lost => {
                ensure_device_available(&self.device_lost)?;
                self.surface = self
                    .instance
                    .create_surface(Arc::clone(&self.window))
                    .map_err(display_error("cannot recover the Venus GPU surface"))?;
                self.surface.configure(&self.device, &self.config);
                ensure_device_available(&self.device_lost)?;
                self.reset_cursor_animation();
                return Ok(PresentOutcome::Recovered);
            }
            CurrentSurfaceTexture::Validation => {
                return Err(RenderError("the GPU rejected the Venus surface".into()));
            }
        };
        let view = frame.texture.create_view(&TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("venus frame encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("venus frame"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(self.clear),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let cursor_boundary = cursor_vertex_boundary(
                self.vertex_count,
                self.cursor_vertex_boundary,
                self.dynamic_vertex_count > 0,
            );
            if self.vertex_count > 0 || self.dynamic_vertex_count > 0 {
                pass.set_pipeline(&self.rectangle_pipeline);
            }
            if cursor_boundary > 0 {
                pass.set_vertex_buffer(0, self.vertices.slice(..));
                pass.draw(0..cursor_boundary, 0..1);
            }
            if self.dynamic_vertex_count > 0 {
                pass.set_vertex_buffer(0, self.dynamic_vertices.slice(..));
                pass.draw(0..self.dynamic_vertex_count, 0..1);
            }
            if cursor_boundary < self.vertex_count {
                pass.set_vertex_buffer(0, self.vertices.slice(..));
                pass.draw(cursor_boundary..self.vertex_count, 0..1);
            }
            self.text_renderer
                .render(&self.atlas, &self.viewport, &mut pass)
                .map_err(display_error("cannot render the Venus glyph atlas"))?;
        }
        self.queue.submit(Some(encoder.finish()));
        ensure_device_available(&self.device_lost)?;
        self.window.pre_present_notify();
        self.queue.present(frame);
        ensure_device_available(&self.device_lost)?;
        self.atlas.trim();
        Ok(PresentOutcome::Presented)
    }

    #[allow(clippy::too_many_arguments)]
    fn rebuild_if_needed(
        &mut self,
        scene: Option<&Scene>,
        scroll_preview: Option<&ScenePreview>,
        scroll_offset: f32,
        workspace: Option<&WorkspaceScene>,
        workspace_focus: WorkspaceFocus,
        status: &str,
        blink_visible: bool,
        preedit: &str,
        generation: u64,
    ) -> bool {
        let key = ContentKey {
            generation,
            scroll_offset,
            workspace_focus,
            blink_visible,
            preedit: preedit.to_owned(),
            status: status.to_owned(),
            hyperlink: self.hyperlink,
            hovered_header: self.hovered_header.clone(),
        };
        if self.content_key.as_ref() == Some(&key) {
            return false;
        }

        self.text.clear();
        self.tab_tooltip = None;
        let mut rectangles = RectangleBatch::new(self.config.width, self.config.height);
        let mut cursor_vertex_boundary = 0;
        if let Some(workspace) = workspace {
            self.clear = wgpu::Color::TRANSPARENT;
            self.build_workspace(workspace, workspace_focus, &mut rectangles);
            if let Some(terminal) = workspace.visible_terminal() {
                rectangles.push(
                    terminal.left,
                    terminal.top,
                    terminal.width,
                    terminal.height,
                    scene.map_or(DEFAULT_BACKGROUND, |scene| scene.background),
                    self.background_opacity,
                );
                if let Some(scene) = scene {
                    let clip = scene_grid(scene, workspace.terminal, self.metrics)
                        .intersection(terminal)
                        .unwrap_or(terminal);
                    rectangles.clip = Some(clip);
                    let origin = shifted(workspace.terminal, scroll_offset);
                    self.build_scene_text(scene, blink_visible, origin, clip);
                    self.build_preview_text(scene, scroll_preview, blink_visible, origin, clip);
                    cursor_vertex_boundary = build_scene_rectangles(
                        &mut rectangles,
                        scene,
                        blink_visible,
                        self.metrics,
                        origin,
                        self.cursor_tail.is_none(),
                    );
                    build_preview_rectangles(
                        &mut rectangles,
                        scene,
                        scroll_preview,
                        blink_visible,
                        self.metrics,
                        origin,
                    );
                    self.build_preedit(scene, preedit, &mut rectangles, origin, clip);
                    self.build_hyperlink(scene, &mut rectangles, origin);
                    rectangles.clip = None;
                }
            }
            rectangles.clip = Some(workspace.pane_viewport);
            if self.pane_frames {
                for pane in &workspace.panes {
                    rectangles.push_rounded_outline(
                        pane_chrome_rect(workspace.pane_bounds(pane), self.metrics),
                        self.metrics.padding,
                        if pane.selected {
                            SceneColor {
                                r: 58,
                                g: 75,
                                b: 91,
                            }
                        } else {
                            SceneColor {
                                r: 43,
                                g: 53,
                                b: 67,
                            }
                        },
                    );
                }
            }
            if workspace_focus == WorkspaceFocus::Panes
                && let Some(pane) = workspace.panes.iter().find(|pane| pane.selected)
            {
                rectangles.push_rounded_outline(
                    pane_chrome_rect(pane.rect, self.metrics),
                    self.metrics.padding,
                    SceneColor {
                        r: 126,
                        g: 231,
                        b: 185,
                    },
                );
            }
            rectangles.clip = None;
            if workspace.directory_picker()
                && workspace_focus == WorkspaceFocus::Terminal
                && let Some(terminal) = workspace.visible_terminal()
            {
                rectangles.push_hollow(
                    terminal.left,
                    terminal.top,
                    terminal.width,
                    terminal.height,
                    1.0,
                    SceneColor {
                        r: 58,
                        g: 75,
                        b: 91,
                    },
                );
            }
            if !status.is_empty() {
                self.build_notice(status, &mut rectangles, Some(workspace));
            }
            self.build_tab_tooltip(workspace, &mut rectangles);
        } else if let Some(scene) = scene {
            self.clear = clear_color(
                scene.background,
                self.background_opacity,
                self.config.format.is_srgb(),
            );
            let viewport = SceneRect {
                left: 0.0,
                top: 0.0,
                width: self.config.width as f32,
                height: self.config.height as f32,
            };
            let clip = scene_grid(scene, viewport, self.metrics);
            rectangles.clip = Some(clip);
            let origin = shifted(viewport, scroll_offset);
            self.build_scene_text(scene, blink_visible, origin, clip);
            self.build_preview_text(scene, scroll_preview, blink_visible, origin, clip);
            cursor_vertex_boundary = build_scene_rectangles(
                &mut rectangles,
                scene,
                blink_visible,
                self.metrics,
                origin,
                self.cursor_tail.is_none(),
            );
            build_preview_rectangles(
                &mut rectangles,
                scene,
                scroll_preview,
                blink_visible,
                self.metrics,
                origin,
            );
            self.build_preedit(scene, preedit, &mut rectangles, origin, clip);
            self.build_hyperlink(scene, &mut rectangles, origin);
            rectangles.clip = None;
            if !status.is_empty() {
                self.build_notice(status, &mut rectangles, None);
            }
        } else {
            self.clear = clear_color(
                DEFAULT_BACKGROUND,
                self.background_opacity,
                self.config.format.is_srgb(),
            );
            self.push_text(
                "VENUS",
                self.metrics.padding * 2.0,
                self.config.height as f32 * 0.42,
                self.config.width as f32 - self.metrics.padding * 4.0,
                self.metrics.height * 1.5,
                SceneColor {
                    r: 126,
                    g: 231,
                    b: 185,
                },
                DrawStyleKind::Heading,
            );
            self.push_text(
                status,
                self.metrics.padding * 2.0,
                self.config.height as f32 * 0.50,
                self.config.width as f32 - self.metrics.padding * 4.0,
                self.metrics.height * 3.0,
                SceneColor {
                    r: 196,
                    g: 202,
                    b: 216,
                },
                DrawStyleKind::Status(Wrap::WordOrGlyph),
            );
        }
        self.cursor_vertex_boundary = cursor_vertex_boundary;
        self.upload_vertices(&rectangles.bytes);
        self.content_key = Some(key);
        true
    }

    fn rebuild_dynamic_cursor(
        &mut self,
        scene: Option<&Scene>,
        workspace: Option<&WorkspaceScene>,
        blink_visible: bool,
    ) {
        let Some((trail_color, duration_scale)) = self.cursor_tail else {
            self.reset_cursor_animation();
            return;
        };
        let (viewport, clip, route) = if let Some(workspace) = workspace {
            let Some(clip) = workspace.visible_terminal() else {
                self.reset_cursor_animation();
                return;
            };
            (
                workspace.terminal,
                Some(clip),
                workspace
                    .panes
                    .iter()
                    .find(|pane| pane.selected)
                    .map(|pane| pane.id.as_str()),
            )
        } else {
            (
                SceneRect {
                    left: 0.0,
                    top: 0.0,
                    width: self.config.width as f32,
                    height: self.config.height as f32,
                },
                None,
                None,
            )
        };
        let now = Instant::now();
        let delta = self
            .last_cursor_frame
            .replace(now)
            .map_or(0.0, |last| now.duration_since(last).as_secs_f32());
        let mut rectangles = RectangleBatch::new(self.config.width, self.config.height);
        rectangles.clip = clip;
        if let Some(scene) = scene {
            build_tail_cursor(
                &mut rectangles,
                &mut self.cursor_animation,
                scene,
                blink_visible,
                self.metrics,
                viewport,
                route,
                trail_color,
                duration_scale,
                delta,
            );
        } else {
            self.cursor_animation.reset();
        }
        self.upload_dynamic_vertices(&rectangles.bytes);
    }

    fn build_tab_tooltip(&mut self, workspace: &WorkspaceScene, rectangles: &mut RectangleBatch) {
        let Some(tab) = workspace
            .tabs
            .iter()
            .find(|tab| self.header_hovered(WorkspaceFocus::Tabs, &tab.id))
        else {
            return;
        };
        let padding = self.metrics.padding;
        let top = workspace.tab_viewport.bottom() + padding / 3.0;
        let width = (self.config.width as f32 - padding * 2.0).min(self.metrics.font_size * 40.0);
        let height = self.config.height as f32 - top - padding;
        if width <= padding * 2.0 || height <= padding * 2.0 {
            return;
        }
        let before = self.text.len();
        self.push_text(
            tab.accessible_label(),
            0.0,
            top + padding,
            width - padding * 2.0,
            height - padding * 2.0,
            SceneColor {
                r: 239,
                g: 244,
                b: 248,
            },
            DrawStyleKind::Status(Wrap::WordOrGlyph),
        );
        let Some(text) = self.text.get_mut(before) else {
            return;
        };
        let mut text_width = 0.0_f32;
        let mut text_height = 0.0_f32;
        for run in text.buffer.layout_runs() {
            text_width = text_width.max(run.line_w);
            text_height = text_height.max(run.line_top + run.line_height);
        }
        let width = (text_width.ceil() + padding * 2.0).min(width);
        let height = (text_height.ceil() + padding * 2.0).min(height);
        let left = tab.rect.left.clamp(
            padding,
            (self.config.width as f32 - width - padding).max(padding),
        );
        text.left = left + padding;
        text.bound_left = text.left.floor() as i32;
        text.right = (left + width - padding).ceil() as i32;
        text.bound_top = (top + padding).floor() as i32;
        text.bottom = (top + height - padding).ceil() as i32;
        self.tab_tooltip = Some((
            before,
            TextBounds {
                left: left.floor() as i32,
                top: top.floor() as i32,
                right: (left + width).ceil() as i32,
                bottom: (top + height).ceil() as i32,
            },
        ));
        rectangles.push_rounded(
            SceneRect {
                left,
                top,
                width,
                height,
            },
            padding / 2.0,
            SceneColor {
                r: 37,
                g: 49,
                b: 64,
            },
        );
    }

    fn build_workspace(
        &mut self,
        workspace: &WorkspaceScene,
        focus: WorkspaceFocus,
        rectangles: &mut RectangleBatch,
    ) {
        let accent = SceneColor {
            r: 126,
            g: 231,
            b: 185,
        };
        let selected = SceneColor {
            r: 28,
            g: 43,
            b: 58,
        };
        let idle = SceneColor {
            r: 16,
            g: 22,
            b: 32,
        };
        rectangles.push(
            workspace.tab_viewport.left,
            workspace.tab_viewport.top,
            workspace.tab_viewport.width,
            workspace.tab_viewport.height,
            DEFAULT_BACKGROUND,
            1.0,
        );
        rectangles.clip = Some(workspace.tab_viewport);
        for tab in &workspace.tabs {
            if tab.rect.intersection(workspace.tab_viewport).is_none() {
                continue;
            }
            let radius = tab.rect.height / 2.0;
            let fill = if tab.selected {
                selected
            } else if self.header_hovered(WorkspaceFocus::Tabs, &tab.id) {
                SceneColor {
                    r: 23,
                    g: 34,
                    b: 46,
                }
            } else {
                idle
            };
            rectangles.push_rounded(tab.rect, radius, fill);
            if tab.selected && focus == WorkspaceFocus::Tabs {
                rectangles.push_rounded(tab.rect, radius, accent);
                rectangles.push_rounded(
                    SceneRect {
                        left: tab.rect.left + 1.0,
                        top: tab.rect.top + 1.0,
                        width: tab.rect.width - 2.0,
                        height: tab.rect.height - 2.0,
                    },
                    radius - 1.0,
                    fill,
                );
            }
            self.push_text_clipped(
                tab.label(),
                tab.rect.left + self.metrics.padding,
                tab.rect.top + (tab.rect.height - self.metrics.height) / 2.0,
                tab.rect.width - self.metrics.padding * 2.0,
                tab.rect.width - self.metrics.padding * 2.0,
                self.metrics.height,
                if tab.selected {
                    SceneColor {
                        r: 239,
                        g: 244,
                        b: 248,
                    }
                } else {
                    SceneColor {
                        r: 162,
                        g: 174,
                        b: 190,
                    }
                },
                DrawStyleKind::Status(Wrap::None),
                workspace.tab_viewport,
            );
        }
        rectangles.clip = None;
        if workspace.directory_picker() {
            rectangles.push(
                workspace.pane_viewport.left,
                workspace.pane_viewport.top,
                workspace.pane_viewport.width,
                workspace.pane_viewport.height,
                idle,
                1.0,
            );
        }
        for pane in &workspace.panes {
            let Some(rect) = pane.rect.intersection(workspace.pane_viewport) else {
                continue;
            };
            rectangles.push(
                rect.left,
                rect.top,
                rect.width,
                rect.height,
                DEFAULT_BACKGROUND,
                1.0,
            );
            let header = pane_chrome_rect(pane.rect, self.metrics);
            rectangles.clip = Some(workspace.pane_viewport);
            if self.header_hovered(WorkspaceFocus::Panes, &pane.id) {
                rectangles.push_rounded(header, self.metrics.padding, idle);
            }
            rectangles.clip = None;
            self.push_text_clipped(
                pane.label(),
                pane.rect.left + self.metrics.padding,
                pane.rect.top + (pane.rect.height - self.metrics.height) / 2.0,
                pane.rect.width - self.metrics.padding * 2.0,
                pane.rect.width - self.metrics.padding * 2.0,
                self.metrics.height,
                if !pane.live {
                    SceneColor {
                        r: 221,
                        g: 126,
                        b: 126,
                    }
                } else if pane.selected {
                    SceneColor {
                        r: 239,
                        g: 244,
                        b: 248,
                    }
                } else {
                    SceneColor {
                        r: 162,
                        g: 174,
                        b: 190,
                    }
                },
                DrawStyleKind::Status(Wrap::None),
                workspace.pane_viewport,
            );
        }
    }

    fn build_hyperlink(&self, scene: &Scene, rectangles: &mut RectangleBatch, origin: SceneRect) {
        if let Some((row, column)) = self.hyperlink
            && let Some(link) = scene.hyperlink_at(row, column)
        {
            let rect = link.rect(origin, self.metrics);
            rectangles.push_hollow(
                rect.left,
                rect.top,
                rect.width,
                rect.height,
                1.0,
                SceneColor {
                    r: 137,
                    g: 180,
                    b: 250,
                },
            );
        }
    }

    fn build_notice(
        &mut self,
        status: &str,
        rectangles: &mut RectangleBatch,
        workspace: Option<&WorkspaceScene>,
    ) {
        let rect = self.notice_rect(workspace);
        // Text is drawn after rectangles; clip underlying rows out of this overlay.
        for text in &mut self.text {
            if text.top < rect.top {
                text.bottom = text.bottom.min(rect.top as i32);
            } else {
                text.bound_top = text.bound_top.max(rect.bottom() as i32);
            }
        }
        rectangles.push(
            rect.left,
            rect.top,
            rect.width,
            rect.height,
            SceneColor {
                r: 35,
                g: 29,
                b: 18,
            },
            0.96,
        );
        self.push_text(
            status,
            self.metrics.padding * 2.0,
            rect.top + self.metrics.height * 0.3,
            self.config.width as f32 - self.metrics.padding * 4.0,
            rect.height - self.metrics.height * 0.3,
            SceneColor {
                r: 245,
                g: 192,
                b: 94,
            },
            if self.hyperlink.is_some() {
                DrawStyleKind::Link
            } else {
                DrawStyleKind::Status(Wrap::None)
            },
        );
    }

    fn build_scene_text(
        &mut self,
        scene: &Scene,
        blink_visible: bool,
        origin: SceneRect,
        clip: SceneRect,
    ) {
        let mut runs = scene.glyph_runs();
        runs.retain(|run| run.style.foreground_visible(blink_visible));
        for (index, run) in runs.iter().enumerate() {
            if is_full_block_run(run) {
                continue;
            }
            let left =
                origin.left + self.metrics.padding + f32::from(run.column) * self.metrics.width;
            let top = origin.top + self.metrics.padding + f32::from(run.row) * self.metrics.height;
            let width = f32::from(run.columns) * self.metrics.width;
            let next = runs.get(index + 1).filter(|next| next.row == run.row);
            let ink_width = f32::from(cell_ink_right(run, next, scene.columns) - run.column)
                * self.metrics.width;
            self.push_text_clipped(
                &run.text,
                left,
                top,
                width,
                ink_width,
                self.metrics.height,
                run.style.foreground,
                DrawStyleKind::Cell(run.style),
                clip,
            );
        }
    }

    fn build_preview_text(
        &mut self,
        scene: &Scene,
        preview: Option<&ScenePreview>,
        blink_visible: bool,
        origin: SceneRect,
        clip: SceneRect,
    ) {
        let Some((direction, rows)) = preview_rows(preview) else {
            return;
        };
        let mut runs = Vec::new();
        for (row_index, row) in rows.iter().enumerate() {
            let top = preview_row_top(scene, direction, row_index, self.metrics, origin);
            if !row_intersects_clip(top, self.metrics.height, clip) {
                continue;
            }
            runs.clear();
            row.append_glyph_runs(0, &mut runs);
            runs.retain(|run| run.style.foreground_visible(blink_visible));
            for (index, run) in runs.iter().enumerate() {
                if is_full_block_run(run) {
                    continue;
                }
                let left =
                    origin.left + self.metrics.padding + f32::from(run.column) * self.metrics.width;
                let width = f32::from(run.columns) * self.metrics.width;
                let next = runs.get(index + 1);
                let ink_width = f32::from(cell_ink_right(run, next, scene.columns) - run.column)
                    * self.metrics.width;
                self.push_text_clipped(
                    &run.text,
                    left,
                    top,
                    width,
                    ink_width,
                    self.metrics.height,
                    run.style.foreground,
                    DrawStyleKind::Cell(run.style),
                    clip,
                );
            }
        }
    }

    fn build_preedit(
        &mut self,
        scene: &Scene,
        preedit: &str,
        rectangles: &mut RectangleBatch,
        origin: SceneRect,
        clip: SceneRect,
    ) {
        let Some(cursor) = scene.cursor.filter(|_| !preedit.is_empty()) else {
            return;
        };
        let left = origin.left
            + self.metrics.padding
            + f32::from(cursor.leading_column()) * self.metrics.width;
        let top = origin.top + self.metrics.padding + f32::from(cursor.row) * self.metrics.height;
        let width = (origin.right() - left - self.metrics.padding).max(1.0);
        let preedit_width = self.push_text_clipped(
            preedit,
            left,
            top,
            width,
            width,
            self.metrics.height,
            scene.foreground,
            DrawStyleKind::Preedit,
            clip,
        );
        rectangles.push(
            left,
            top + self.metrics.height - 2.0,
            preedit_width.max(self.metrics.width).min(width),
            2.0,
            scene.foreground,
            1.0,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn push_text_clipped(
        &mut self,
        text: &str,
        left: f32,
        top: f32,
        layout_width: f32,
        ink_width: f32,
        layout_height: f32,
        foreground: SceneColor,
        kind: DrawStyleKind,
        clip: SceneRect,
    ) -> f32 {
        let Some(bounds) = (SceneRect {
            left,
            top,
            width: ink_width.max(1.0),
            height: layout_height.max(1.0),
        })
        .intersection(clip) else {
            return 0.0;
        };
        let previous_len = self.text.len();
        let width = self.push_text(
            text,
            left,
            top,
            layout_width,
            layout_height,
            foreground,
            kind,
        );
        if self.text.len() > previous_len {
            for text in &mut self.text[previous_len..] {
                text.bound_left = bounds.left.floor() as i32;
                text.bound_top = bounds.top.floor() as i32;
                text.right = bounds.right().ceil() as i32;
                text.bottom = bounds.bottom().ceil() as i32;
            }
        }
        width
    }

    #[allow(clippy::too_many_arguments)]
    fn push_text(
        &mut self,
        text: &str,
        left: f32,
        top: f32,
        layout_width: f32,
        layout_height: f32,
        foreground: SceneColor,
        kind: DrawStyleKind,
    ) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        let (font_size, line_height, attrs, monospace_width, wrap, alpha) = match kind {
            DrawStyleKind::Cell(style) => {
                self.push_cell_text(
                    text,
                    left,
                    top,
                    layout_width,
                    layout_height,
                    foreground,
                    style,
                );
                return 0.0;
            }
            DrawStyleKind::Heading => (
                self.metrics.font_size * 1.15,
                self.metrics.height * 1.4,
                Attrs::new().family(Family::SansSerif).weight(Weight::BOLD),
                None,
                Wrap::None,
                255,
            ),
            DrawStyleKind::Preedit | DrawStyleKind::Link => (
                self.cell_font.size,
                self.metrics.height,
                self.cell_font.attrs(),
                Some(self.metrics.width),
                Wrap::None,
                255,
            ),
            DrawStyleKind::Status(wrap) => (
                self.metrics.font_size,
                if wrap == Wrap::None {
                    self.metrics.height
                } else {
                    self.metrics.height * 1.25
                },
                Attrs::new().family(Family::SansSerif),
                None,
                wrap,
                255,
            ),
        };
        let layout_width = layout_width.max(1.0);
        let layout_height = layout_height.max(1.0);
        let mut buffer = Buffer::new(
            &mut self.fonts.font_system,
            Metrics::new(font_size, line_height),
        );
        buffer.set_size(Some(layout_width), Some(layout_height));
        buffer.set_wrap(wrap);
        buffer.set_monospace_width(monospace_width);
        buffer.set_text(text, &attrs, shaping(text), None);
        buffer.shape_until_scroll(&mut self.fonts.font_system, false);
        let (left_offset, measured_width) = if matches!(kind, DrawStyleKind::Preedit) {
            shaped_preedit_placement(&buffer)
        } else {
            (0.0, 0.0)
        };
        self.text.push(PlacedText {
            buffer,
            left: left + left_offset,
            top: top
                - if matches!(kind, DrawStyleKind::Preedit | DrawStyleKind::Link) {
                    self.cell_font.top_offset
                } else {
                    0.0
                },
            right: (left + layout_width).ceil() as i32,
            bottom: (top + layout_height).ceil() as i32,
            bound_left: left.floor() as i32,
            bound_top: top.floor() as i32,
            color: Color::rgba(foreground.r, foreground.g, foreground.b, alpha),
        });
        measured_width
    }

    #[allow(clippy::too_many_arguments)]
    fn push_cell_text(
        &mut self,
        text: &str,
        left: f32,
        top: f32,
        layout_width: f32,
        layout_height: f32,
        foreground: SceneColor,
        style: DrawStyle,
    ) {
        let segments = cell_text_segments(text);
        let attrs = cell_text_attrs(self.cell_font, text, style);
        let box_attrs = attrs
            .clone()
            .metrics(Metrics::new(self.cell_font.box_size, self.metrics.height))
            .letter_spacing(self.cell_font.box_letter_spacing);
        let braille_attrs = if self.fonts.family.is_some() {
            attrs.clone()
        } else {
            attrs.clone().family(Family::Name(BRAILLE_FAMILY))
        };
        for box_drawing in [false, true] {
            if !segments
                .iter()
                .any(|(_, kind)| (*kind == CellTextKind::Box) == box_drawing)
            {
                continue;
            }
            // A cell needs one line, not Vec's four-line initial allocation.
            let mut buffer =
                Buffer::new_empty(Metrics::new(self.cell_font.size, self.metrics.height));
            buffer.lines.reserve_exact(1);
            buffer.set_size(Some(layout_width.max(1.0)), Some(layout_height.max(1.0)));
            buffer.set_wrap(Wrap::None);
            buffer.set_monospace_width(Some(self.metrics.width));
            set_cell_layer(
                &mut buffer,
                text,
                &segments,
                &attrs,
                &braille_attrs,
                &box_attrs,
                box_drawing,
            );
            buffer.shape_until_scroll(&mut self.fonts.font_system, false);
            self.text.push(PlacedText {
                buffer,
                left,
                top: top
                    - if box_drawing {
                        0.0
                    } else {
                        self.cell_font.top_offset
                    },
                right: (left + layout_width).ceil() as i32,
                bottom: (top + layout_height).ceil() as i32,
                bound_left: left.floor() as i32,
                bound_top: top.floor() as i32,
                color: Color::rgba(
                    foreground.r,
                    foreground.g,
                    foreground.b,
                    style.foreground_alpha(),
                ),
            });
        }
    }

    fn upload_vertices(&mut self, bytes: &[u8]) {
        self.vertex_count = upload_vertices(
            &self.device,
            &self.queue,
            &mut self.vertices,
            &mut self.vertex_capacity,
            bytes,
            "venus rectangle vertices",
        );
    }

    fn upload_dynamic_vertices(&mut self, bytes: &[u8]) {
        self.dynamic_vertex_count = upload_vertices(
            &self.device,
            &self.queue,
            &mut self.dynamic_vertices,
            &mut self.dynamic_vertex_capacity,
            bytes,
            "venus dynamic cursor vertices",
        );
    }
}

fn vertex_count(bytes: &[u8]) -> u32 {
    u32::try_from(bytes.len() / VERTEX_SIZE as usize)
        .expect("bounded draw inputs fit the vertex count")
}

fn cursor_vertex_boundary(total: u32, boundary: u32, dynamic: bool) -> u32 {
    (if dynamic { boundary } else { total }).min(total)
}

fn upload_vertices(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    buffer: &mut wgpu::Buffer,
    capacity: &mut u64,
    bytes: &[u8],
    label: &'static str,
) -> u32 {
    let count = vertex_count(bytes);
    if bytes.is_empty() {
        return count;
    }
    let needed = bytes.len() as u64;
    if needed > *capacity {
        *capacity = needed.next_power_of_two();
        *buffer = device.create_buffer(&BufferDescriptor {
            label: Some(label),
            size: *capacity,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
    }
    queue.write_buffer(buffer, 0, bytes);
    count
}

#[derive(Clone, Copy)]
enum DrawStyleKind {
    Cell(DrawStyle),
    Heading,
    Preedit,
    Link,
    Status(Wrap),
}

fn shaped_preedit_placement(buffer: &Buffer) -> (f32, f32) {
    let mut glyphs = buffer.layout_runs().flat_map(|run| run.glyphs);
    let Some(first) = glyphs.next() else {
        return (0.0, 0.0);
    };
    let (left, right) = glyphs.fold((first.x, first.x + first.w), |(left, right), glyph| {
        (left.min(glyph.x), right.max(glyph.x + glyph.w))
    });
    (-left, right - left)
}

fn fitted_cell_font(
    font_system: &mut FontSystem,
    metrics: CellMetrics,
    family: Option<&'static str>,
    configured: bool,
) -> CellFont {
    let top_offset = ((metrics.height - metrics.font_size) / 2.0)
        .round()
        .max(0.0);
    let mut buffer = Buffer::new(font_system, Metrics::new(metrics.font_size, metrics.height));
    buffer.set_wrap(Wrap::None);
    buffer.set_text(
        " ",
        &Attrs::new().family(family.map_or(Family::Monospace, Family::Name)),
        Shaping::Advanced,
        None,
    );
    let advance = buffer
        .line_layout(font_system, 0)
        .and_then(|lines| lines.first())
        .map(|line| line.w)
        .filter(|width| width.is_finite() && *width > 0.0);
    advance.map_or(
        CellFont {
            family,
            size: metrics.font_size,
            top_offset,
            letter_spacing: 0.0,
            box_size: metrics.font_size,
            box_letter_spacing: 0.0,
        },
        |advance| {
            let fitted_size = (metrics.font_size * metrics.width / advance)
                .round()
                .max(1.0);
            let size = (fitted_size - 1.0).clamp(1.0, metrics.font_size);
            let box_size = if configured {
                (fitted_size + 1.0).max(metrics.height)
            } else {
                fitted_size + 1.0
            };
            CellFont {
                family,
                size,
                top_offset,
                letter_spacing: metrics.width / size - advance / metrics.font_size,
                box_size,
                box_letter_spacing: metrics.width / box_size - advance / metrics.font_size,
            }
        },
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CellTextKind {
    Text,
    Braille,
    Box,
}

fn cell_text_kind(character: char) -> CellTextKind {
    match character {
        '\u{2500}'..='\u{257f}' => CellTextKind::Box,
        '\u{2800}'..='\u{28ff}' => CellTextKind::Braille,
        _ => CellTextKind::Text,
    }
}

fn cell_text_segments(text: &str) -> Vec<(&str, CellTextKind)> {
    let Some(first) = text.chars().next() else {
        return Vec::new();
    };
    let mut segments = Vec::new();
    let mut start = 0;
    let mut kind = cell_text_kind(first);
    for (index, character) in text.char_indices().skip(1) {
        let next_kind = cell_text_kind(character);
        if next_kind != kind {
            segments.push((&text[start..index], kind));
            start = index;
            kind = next_kind;
        }
    }
    segments.push((&text[start..], kind));
    segments
}

fn is_nerd_font_symbol(text: &str) -> bool {
    text.chars().any(|character| {
        matches!(
            character,
            '\u{e000}'..='\u{f8ff}' | '\u{f0000}'..='\u{f2fff}'
        )
    })
}

fn is_full_block_run(run: &GlyphRun) -> bool {
    run.columns == 1 && run.text == "█"
}

fn cell_ink_right(run: &GlyphRun, next: Option<&GlyphRun>, columns: u16) -> u16 {
    let cell_right = run.column.saturating_add(run.columns);
    if is_nerd_font_symbol(&run.text) {
        next.map_or(columns, |next| next.column).max(cell_right)
    } else {
        cell_right
    }
}

fn cell_text_attrs(cell_font: CellFont, text: &str, style: DrawStyle) -> Attrs<'static> {
    let mut attrs = cell_font.attrs();
    if is_nerd_font_symbol(text) {
        // The packaged symbol face is Regular-only; bold can select an ambient mono face.
        if cell_font.family.is_none() {
            attrs = attrs.family(Family::Name(NERD_FONT_FAMILY));
        }
        return attrs;
    }
    if style.bold {
        attrs = attrs.weight(Weight::BOLD);
    }
    if style.italic {
        attrs = attrs.style(Style::Italic);
    }
    attrs
}

fn set_cell_layer(
    buffer: &mut Buffer,
    text: &str,
    segments: &[(&str, CellTextKind)],
    attrs: &Attrs<'static>,
    braille_attrs: &Attrs<'static>,
    box_attrs: &Attrs<'static>,
    box_drawing: bool,
) {
    let hidden = attrs.clone().color(Color::rgba(0, 0, 0, 0));
    buffer.set_rich_text(
        segments.iter().map(|(segment, kind)| {
            (
                *segment,
                match (*kind, box_drawing) {
                    (CellTextKind::Text, false) => attrs.clone(),
                    (CellTextKind::Braille, false) => braille_attrs.clone(),
                    (CellTextKind::Box, true) => box_attrs.clone(),
                    _ => hidden.clone(),
                },
            )
        }),
        attrs,
        shaping(text),
        None,
    );
}

fn shaping(text: &str) -> Shaping {
    if text.is_ascii() {
        Shaping::Basic
    } else {
        Shaping::Advanced
    }
}

fn fit_tab_text(
    fonts: &mut FontSystem,
    metrics: CellMetrics,
    text: &str,
    width: f32,
) -> (String, f32) {
    let mut buffer = Buffer::new(fonts, Metrics::new(metrics.font_size, metrics.height));
    buffer.set_size(None, Some(metrics.height));
    buffer.set_wrap(Wrap::None);
    let measure = |buffer: &mut Buffer, fonts: &mut FontSystem, text: &str| {
        buffer.set_text(
            text,
            &Attrs::new().family(Family::SansSerif),
            shaping(text),
            None,
        );
        buffer.shape_until_scroll(fonts, false);
        buffer
            .layout_runs()
            .map(|run| run.line_w)
            .fold(0.0, f32::max)
    };
    let measured = measure(&mut buffer, fonts, text);
    if measured <= width {
        return (text.to_owned(), measured);
    }

    // Keep the numeric identity; cuts use the shaper's original cluster boundaries.
    let prefix = text.find("  ").map_or(0, |index| index + 2);
    let mut boundaries: Vec<_> = buffer
        .layout_runs()
        .flat_map(|run| run.glyphs.iter())
        .flat_map(|glyph| [glyph.start, glyph.end])
        .filter(|index| *index >= prefix)
        .collect();
    boundaries.extend([prefix, text.len()]);
    boundaries.sort_unstable();
    boundaries.dedup();
    let mut low = 0;
    let mut high = boundaries.len().saturating_sub(2);
    let identity = text[..prefix].trim_end();
    let identity_width = measure(&mut buffer, fonts, identity);
    let mut best = if identity_width <= width {
        (identity.to_owned(), identity_width)
    } else {
        (String::new(), 0.0)
    };
    while low <= high {
        let kept = low + (high - low) / 2;
        let candidate = format!(
            "{}…{}",
            &text[..boundaries[kept.div_ceil(2)]],
            &text[boundaries[boundaries.len() - 1 - kept / 2]..]
        );
        let measured = measure(&mut buffer, fonts, &candidate);
        if measured <= width {
            best = (candidate, measured);
            low = kept + 1;
        } else if kept == 0 {
            break;
        } else {
            high = kept - 1;
        }
    }
    best
}

fn text_areas(
    text: &[PlacedText],
    overlay: Option<(usize, TextBounds)>,
) -> impl Iterator<Item = TextArea<'_>> {
    text.iter().enumerate().flat_map(move |(index, text)| {
        let bounds = TextBounds {
            left: text.bound_left,
            top: text.bound_top,
            right: text.right,
            bottom: text.bottom,
        };
        let mut clips = [bounds; 4];
        let count = if let Some((first_overlay_text, occlusion)) = overlay
            && index < first_overlay_text
            && bounds.left < occlusion.right
            && bounds.right > occlusion.left
            && bounds.top < occlusion.bottom
            && bounds.bottom > occlusion.top
        {
            // Rectangles precede glyphs. Keep the text around this floating
            // preview without cloning its buffers or adding a rendering pass.
            clips[0].bottom = bounds.bottom.min(occlusion.top);
            clips[1].top = bounds.top.max(occlusion.bottom);
            clips[2].top = bounds.top.max(occlusion.top);
            clips[2].bottom = bounds.bottom.min(occlusion.bottom);
            clips[2].right = bounds.right.min(occlusion.left);
            clips[3] = TextBounds {
                left: bounds.left.max(occlusion.right),
                right: bounds.right,
                ..clips[2]
            };
            4
        } else {
            1
        };
        clips
            .into_iter()
            .take(count)
            .filter(|clip| clip.left < clip.right && clip.top < clip.bottom)
            .map(move |bounds| TextArea {
                buffer: &text.buffer,
                left: text.left,
                top: text.top,
                scale: 1.0,
                bounds,
                default_color: text.color,
                custom_glyphs: &[],
            })
    })
}

struct RectangleBatch {
    bytes: Vec<u8>,
    width: u32,
    height: u32,
    clip: Option<SceneRect>,
}

fn pane_chrome_rect(rect: SceneRect, metrics: CellMetrics) -> SceneRect {
    let inset = (metrics.padding / 3.0)
        .min(rect.width / 4.0)
        .min(metrics.height / 4.0);
    SceneRect {
        left: rect.left + inset,
        top: rect.top + inset / 2.0,
        width: (rect.width - inset * 2.0).max(0.0),
        height: (rect.height - inset).max(0.0),
    }
}

impl RectangleBatch {
    /// A one-physical-pixel stroke, leaving the translucent interior untouched.
    fn push_rounded_outline(&mut self, rect: SceneRect, radius: f32, color: SceneColor) {
        if self
            .clip
            .is_some_and(|clip| rect.intersection(clip).is_none())
        {
            return;
        }
        if rect.width <= 2.0 || rect.height <= 2.0 {
            self.push_rounded(rect, radius, color);
            return;
        }
        let radius = radius
            .min(rect.width / 2.0)
            .min(rect.height / 2.0)
            .max(0.0)
            .floor();
        if radius < 1.0 {
            self.push_hollow(rect.left, rect.top, rect.width, rect.height, 1.0, color);
            return;
        }
        for left in [rect.left, rect.right() - 1.0] {
            self.push(
                left,
                rect.top + radius,
                1.0,
                rect.height - radius * 2.0,
                color,
                1.0,
            );
        }
        for row in 0..radius as u32 {
            let dy = radius - row as f32 - 0.5;
            let outer = radius - (radius * radius - dy * dy).sqrt();
            for top in [rect.top + row as f32, rect.bottom() - row as f32 - 1.0] {
                if row == 0 {
                    self.push_span(rect.left + outer, rect.right() - outer, top, color);
                } else {
                    let inner = radius - ((radius - 1.0).powi(2) - dy * dy).sqrt();
                    self.push_span(rect.left + outer, rect.left + inner, top, color);
                    self.push_span(rect.right() - inner, rect.right() - outer, top, color);
                }
            }
        }
    }

    fn push_span(&mut self, left: f32, right: f32, top: f32, color: SceneColor) {
        if left.floor() == right.floor() {
            self.push(left.floor(), top, 1.0, 1.0, color, right - left);
        } else {
            self.push(left.floor(), top, 1.0, 1.0, color, left.ceil() - left);
            self.push(
                left.ceil(),
                top,
                right.floor() - left.ceil(),
                1.0,
                color,
                1.0,
            );
            self.push(right.floor(), top, 1.0, 1.0, color, right - right.floor());
        }
    }

    fn push_rounded(&mut self, rect: SceneRect, radius: f32, color: SceneColor) {
        let radius = radius
            .min(rect.width / 2.0)
            .min(rect.height / 2.0)
            .max(0.0)
            .floor();
        self.push(
            rect.left,
            rect.top + radius,
            rect.width,
            rect.height - radius * 2.0,
            color,
            1.0,
        );
        // Pixel-height bands reuse the existing clipped pipeline; fractional edge
        // coverage softens the curve without widening every terminal vertex.
        for row in 0..radius as u32 {
            let dy = radius - row as f32 - 0.5;
            let inset = radius - (radius * radius - dy * dy).sqrt();
            let left = rect.left + inset;
            let right = rect.right() - inset;
            for top in [rect.top + row as f32, rect.bottom() - row as f32 - 1.0] {
                self.push(
                    left.ceil(),
                    top,
                    right.floor() - left.ceil(),
                    1.0,
                    color,
                    1.0,
                );
                self.push(left.floor(), top, 1.0, 1.0, color, left.ceil() - left);
                self.push(right.floor(), top, 1.0, 1.0, color, right - right.floor());
            }
        }
    }

    fn new(width: u32, height: u32) -> Self {
        Self {
            bytes: Vec::new(),
            width,
            height,
            clip: None,
        }
    }

    fn push(&mut self, x: f32, y: f32, width: f32, height: f32, color: SceneColor, alpha: f32) {
        let mut rect = SceneRect {
            left: x,
            top: y,
            width,
            height,
        };
        if let Some(clip) = self.clip {
            let Some(clipped) = rect.intersection(clip) else {
                return;
            };
            rect = clipped;
        }
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        let [top_left, top_right, bottom_right, bottom_left] = rect_corners(rect);
        self.push_points(
            [
                top_left,
                bottom_left,
                bottom_right,
                top_left,
                bottom_right,
                top_right,
            ],
            color,
            alpha,
        );
    }

    fn push_quad(&mut self, points: [CursorPoint; 4], color: SceneColor, alpha: f32) {
        if !alpha.is_finite()
            || !points
                .iter()
                .all(|point| point.x.is_finite() && point.y.is_finite())
        {
            return;
        }
        let points = points.map(|mut point| {
            if let Some(clip) = self.clip {
                point.x = point.x.clamp(clip.left, clip.right());
                point.y = point.y.clamp(clip.top, clip.bottom());
            }
            point
        });
        self.push_points(
            [
                points[0], points[1], points[2], points[0], points[2], points[3],
            ],
            color,
            alpha,
        );
    }

    fn push_points(&mut self, points: [CursorPoint; 6], color: SceneColor, alpha: f32) {
        let to_x = |value: f32| value / self.width as f32 * 2.0 - 1.0;
        let to_y = |value: f32| 1.0 - value / self.height as f32 * 2.0;
        let rgba = [
            f32::from(color.r) / 255.0,
            f32::from(color.g) / 255.0,
            f32::from(color.b) / 255.0,
            alpha,
        ];
        for point in points {
            let x = to_x(point.x);
            let y = to_y(point.y);
            for value in [x, y, rgba[0], rgba[1], rgba[2], rgba[3]] {
                self.bytes.extend_from_slice(&value.to_ne_bytes());
            }
        }
    }

    fn push_hollow(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        thickness: f32,
        color: SceneColor,
    ) {
        for (x, y, width, height) in [
            (x, y, width, thickness),
            (x, y + height - thickness, width, thickness),
            (x, y, thickness, height),
            (x + width - thickness, y, thickness, height),
        ] {
            self.push(x, y, width, height, color, 1.0);
        }
    }
}

fn build_scene_rectangles(
    rectangles: &mut RectangleBatch,
    scene: &Scene,
    blink_visible: bool,
    metrics: CellMetrics,
    viewport: SceneRect,
    draw_cursor: bool,
) -> u32 {
    for (row_index, row) in scene.content.iter().enumerate() {
        build_row_rectangles(
            rectangles,
            row,
            blink_visible,
            metrics,
            viewport.left + metrics.padding,
            viewport.top + metrics.padding + row_index as f32 * metrics.height,
        );
    }
    if draw_cursor
        && let Some(cursor) = scene
            .cursor
            .filter(|cursor| cursor.visible && (blink_visible || !cursor.blinking))
        && let Some(bounds) = cursor_bounds(scene, cursor, metrics, viewport)
    {
        push_cursor(rectangles, cursor, bounds, metrics);
    }
    vertex_count(&rectangles.bytes)
}

fn build_preview_rectangles(
    rectangles: &mut RectangleBatch,
    scene: &Scene,
    preview: Option<&ScenePreview>,
    blink_visible: bool,
    metrics: CellMetrics,
    origin: SceneRect,
) {
    if let Some((direction, rows)) = preview_rows(preview) {
        for (index, row) in rows.iter().enumerate() {
            let top = preview_row_top(scene, direction, index, metrics, origin);
            if rectangles
                .clip
                .is_some_and(|clip| !row_intersects_clip(top, metrics.height, clip))
            {
                continue;
            }
            build_row_rectangles(
                rectangles,
                row,
                blink_visible,
                metrics,
                origin.left + metrics.padding,
                top,
            );
        }
    }
}

fn build_row_rectangles(
    rectangles: &mut RectangleBatch,
    row: &DrawRow,
    blink_visible: bool,
    metrics: CellMetrics,
    row_left: f32,
    row_top: f32,
) {
    let mut start = 0_usize;
    while start < row.cells.len() {
        let background = row.cells[start].style.background;
        let background_is_default = row.cells[start].style.background_is_default;
        let mut end = start + 1;
        while end < row.cells.len()
            && row.cells[end].style.background == background
            && row.cells[end].style.background_is_default == background_is_default
        {
            end += 1;
        }
        if !background_is_default {
            rectangles.push(
                row_left + start as f32 * metrics.width,
                row_top,
                (end - start) as f32 * metrics.width,
                metrics.height,
                background,
                1.0,
            );
        }
        start = end;
    }
    for (column, cell) in row.cells.iter().enumerate() {
        if !cell.style.foreground_visible(blink_visible) {
            continue;
        }
        let left = row_left + column as f32 * metrics.width;
        let top = row_top;
        if cell.is_full_block() {
            rectangles.push(
                left,
                top,
                metrics.width,
                metrics.height,
                cell.style.foreground,
                f32::from(cell.style.foreground_alpha()) / 255.0,
            );
        }
        let thickness = (metrics.height / 14.0).max(1.0);
        match cell.style.underline {
            Underline::None => {}
            Underline::Single => rectangles.push(
                left,
                top + metrics.height - thickness * 2.0,
                metrics.width,
                thickness,
                cell.style.underline_color,
                1.0,
            ),
            Underline::Curly => {
                let segment = (metrics.width / 4.0).max(1.0);
                for part in 0..4 {
                    rectangles.push(
                        left + part as f32 * segment,
                        top + metrics.height - thickness * if part % 2 == 0 { 3.0 } else { 1.5 },
                        segment,
                        thickness,
                        cell.style.underline_color,
                        1.0,
                    );
                }
            }
            Underline::Dotted => {
                let dot = thickness.max(1.0);
                let mut x = left;
                while x < left + metrics.width {
                    rectangles.push(
                        x,
                        top + metrics.height - thickness * 2.0,
                        dot,
                        thickness,
                        cell.style.underline_color,
                        1.0,
                    );
                    x += dot * 2.0;
                }
            }
            Underline::Dashed => {
                let dash = (metrics.width / 3.0).max(1.0);
                for part in [0.0, 2.0] {
                    rectangles.push(
                        left + part * dash,
                        top + metrics.height - thickness * 2.0,
                        dash,
                        thickness,
                        cell.style.underline_color,
                        1.0,
                    );
                }
            }
            Underline::Double => {
                for offset in [2.0, 4.0] {
                    rectangles.push(
                        left,
                        top + metrics.height - thickness * offset,
                        metrics.width,
                        thickness,
                        cell.style.underline_color,
                        1.0,
                    );
                }
            }
        }
        if cell.style.strikethrough {
            rectangles.push(
                left,
                top + metrics.height * 0.52,
                metrics.width,
                thickness,
                cell.style.foreground,
                1.0,
            );
        }
        if cell.style.overline {
            rectangles.push(
                left,
                top + thickness,
                metrics.width,
                thickness,
                cell.style.foreground,
                1.0,
            );
        }
    }
}

fn cursor_bounds(
    scene: &Scene,
    cursor: DrawCursor,
    metrics: CellMetrics,
    viewport: SceneRect,
) -> Option<SceneRect> {
    if !valid_rect(viewport)
        || !metrics.width.is_finite()
        || !metrics.height.is_finite()
        || !metrics.padding.is_finite()
        || metrics.width <= 0.0
        || metrics.height <= 0.0
        || cursor.row >= scene.rows
    {
        return None;
    }
    let column = cursor.leading_column();
    let wide = cursor.at_wide_tail
        || scene
            .content
            .get(usize::from(cursor.row))
            .and_then(|row| row.cells.get(usize::from(column)))
            .is_some_and(|cell| cell.width == CellWidth::Wide);
    let columns = if wide { 2 } else { 1 };
    if column.checked_add(columns)? > scene.columns {
        return None;
    }
    Some(SceneRect {
        left: viewport.left + metrics.padding + f32::from(column) * metrics.width,
        top: viewport.top + metrics.padding + f32::from(cursor.row) * metrics.height,
        width: metrics.width * f32::from(columns),
        height: metrics.height,
    })
}

fn push_cursor(
    rectangles: &mut RectangleBatch,
    cursor: DrawCursor,
    bounds: SceneRect,
    metrics: CellMetrics,
) {
    let thickness = (metrics.width / 7.0).max(1.0);
    let (x, y, width, height, alpha) = match cursor.shape {
        CursorShape::Bar => (bounds.left, bounds.top, thickness, bounds.height, 1.0),
        CursorShape::Underline => (
            bounds.left,
            bounds.bottom() - thickness,
            bounds.width,
            thickness,
            1.0,
        ),
        CursorShape::Block => (bounds.left, bounds.top, bounds.width, bounds.height, 0.55),
        CursorShape::BlockHollow => {
            rectangles.push_hollow(
                bounds.left,
                bounds.top,
                bounds.width,
                bounds.height,
                thickness,
                cursor.color,
            );
            return;
        }
    };
    rectangles.push(x, y, width, height, cursor.color, alpha);
}

#[allow(clippy::too_many_arguments)]
fn build_tail_cursor(
    rectangles: &mut RectangleBatch,
    animation: &mut CursorAnimation,
    scene: &Scene,
    blink_visible: bool,
    metrics: CellMetrics,
    viewport: SceneRect,
    route: Option<&str>,
    trail_color: SceneColor,
    duration_scale: f32,
    delta: f32,
) -> bool {
    let Some(cursor) = scene
        .cursor
        .filter(|cursor| cursor.visible && (blink_visible || !cursor.blinking))
    else {
        animation.reset();
        return false;
    };
    let Some(bounds) = cursor_bounds(scene, cursor, metrics, viewport) else {
        animation.reset();
        return false;
    };
    let active = animation.update(
        bounds,
        route,
        viewport,
        metrics.width,
        delta,
        duration_scale,
    );
    if active {
        rectangles.push_quad(animation.corners(), trail_color, 1.0);
    }
    push_cursor(rectangles, cursor, bounds, metrics);
    active
}

fn nonzero(size: PhysicalSize<u32>) -> PhysicalSize<u32> {
    PhysicalSize::new(size.width.max(1), size.height.max(1))
}

fn surface_alpha_mode(
    background_opacity: f32,
    supported: &[CompositeAlphaMode],
) -> Result<CompositeAlphaMode, RenderError> {
    if background_opacity == 1.0 {
        return Ok(CompositeAlphaMode::Auto);
    }
    supported
        .contains(&CompositeAlphaMode::PreMultiplied)
        .then_some(CompositeAlphaMode::PreMultiplied)
        .ok_or_else(|| {
            RenderError("the GPU surface does not support premultiplied transparency".into())
        })
}

fn clear_color(color: SceneColor, background_opacity: f32, srgb_target: bool) -> wgpu::Color {
    let channel = |value| {
        let value = f64::from(value) / 255.0;
        let linear = if !srgb_target {
            value
        } else if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        };
        linear * f64::from(background_opacity)
    };
    wgpu::Color {
        r: channel(color.r),
        g: channel(color.g),
        b: channel(color.b),
        a: f64::from(background_opacity),
    }
}

fn display_error<E>(context: &'static str) -> impl FnOnce(E) -> RenderError
where
    E: fmt::Display,
{
    move |error| RenderError(format!("{context}: {error}"))
}

fn record_device_loss(device_lost: &AtomicBool, reason: DeviceLostReason) -> bool {
    if reason == DeviceLostReason::Destroyed {
        return false;
    }
    device_lost.store(true, Ordering::Release);
    true
}

fn ensure_device_available(device_lost: &AtomicBool) -> Result<(), RenderError> {
    if device_lost.load(Ordering::Acquire) {
        Err(RenderError(DEVICE_LOST.into()))
    } else {
        Ok(())
    }
}

fn handle_uncaptured_gpu_error(device_lost: &AtomicBool, error: WgpuError) {
    assert!(device_lost.load(Ordering::Acquire), "wgpu error: {error}\n");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DrawCell, DrawCursor, DrawRow};
    use orbit_protocol::{CellWidth, Screen};

    #[test]
    fn configured_fonts_resolve_fallbacks_and_scale_one_grid() {
        let original = FontSystem::new();
        let named = |name: &str| {
            original
                .db()
                .faces()
                .find(|face| face.families.iter().any(|(family, _)| family == name))
                .expect("the accepted font environment supplies the tested face")
                .clone()
        };
        let mono = named("DejaVu Sans Mono");
        let symbol = named(NERD_FONT_FAMILY);
        let make_system = || {
            let mut db = fontdb::Database::new();
            db.push_face_info(mono.clone());
            for name in ["Venus Fallback A", "Venus Fallback B"] {
                let mut face = symbol.clone();
                face.families.truncate(1);
                face.families[0].0 = name.into();
                db.push_face_info(face);
            }
            FontSystem::new_with_locale_and_db("en-US".into(), db)
        };
        let mut settings = FontSettings {
            family: Some("DejaVu Sans Mono".into()),
            fallbacks: vec!["Venus Fallback A".into(), "Venus Fallback B".into()],
            size: 20.0,
            line_height: 1.5,
        };
        for expected in ["Venus Fallback A", "Venus Fallback B"] {
            let mut fonts = FontSetup::resolve(&settings, make_system()).unwrap();
            for scale in [1.0, 1.25, 1.5, 2.0] {
                let metrics = fonts.metrics(scale);
                assert_eq!(metrics.height, (30.0 * scale as f32).round());
                assert_eq!(metrics.font_size, 20.0 * scale as f32);
                let cell_font =
                    fitted_cell_font(&mut fonts.font_system, metrics, fonts.family, true);
                let mut buffer = Buffer::new(
                    &mut fonts.font_system,
                    Metrics::new(cell_font.size, metrics.height),
                );
                buffer.set_monospace_width(Some(metrics.width));
                buffer.set_text(
                    "\u{f015}",
                    &cell_text_attrs(
                        cell_font,
                        "\u{f015}",
                        DrawStyle {
                            bold: true,
                            ..plain_style()
                        },
                    ),
                    Shaping::Advanced,
                    None,
                );
                buffer.shape_until_scroll(&mut fonts.font_system, false);
                let glyph = &buffer.layout_runs().next().unwrap().glyphs[0];
                assert_ne!(glyph.glyph_id, 0);
                assert_eq!(
                    fonts.font_system.db().face(glyph.font_id).unwrap().families[0].0,
                    expected
                );
                let buffer = grid_buffer(
                    &mut fonts.font_system,
                    metrics,
                    cell_font,
                    "abc",
                    false,
                    cell_font.attrs(),
                );
                for (column, glyph) in buffer
                    .layout_runs()
                    .next()
                    .unwrap()
                    .glyphs
                    .iter()
                    .enumerate()
                {
                    assert!((glyph.x - column as f32 * metrics.width).abs() < 0.01);
                }
            }
            settings.fallbacks.reverse();
        }
        settings.family = Some("Missing Venus Font".into());
        assert!(
            FontSetup::resolve(&settings, make_system())
                .err()
                .unwrap()
                .to_string()
                .contains("unavailable")
        );
        settings.family = None;
        settings.fallbacks = vec!["Missing Venus Fallback".into()];
        assert!(
            FontSetup::resolve(&settings, make_system())
                .err()
                .unwrap()
                .to_string()
                .contains("unavailable")
        );
        settings.family = Some("DejaVu Sans".into());
        settings.fallbacks.clear();
        assert!(
            FontSetup::resolve(&settings, FontSystem::new())
                .err()
                .unwrap()
                .to_string()
                .contains("not monospace")
        );
        let defaults = FontSetup::new(&FontSettings::default()).unwrap();
        for scale in [1.0, 1.25, 1.5, 2.0] {
            assert_eq!(defaults.metrics(scale), CellMetrics::for_scale(scale));
        }
    }

    #[test]
    fn tab_text_fits_shaped_width_without_splitting_combining_clusters() {
        let mut fonts = FontSystem::new();
        for scale in [1.0, 1.25, 2.0] {
            let metrics = CellMetrics::for_scale(scale);
            let available = (crate::scene::tab_max_width(metrics, 1000.0 * scale as f32)
                - metrics.padding * 2.0)
                .floor();
            for label in ["1  eon", "2  machine_vs_aliens"] {
                let fitted = fit_tab_text(&mut fonts, metrics, label, available);
                assert_eq!(fitted.0, label);
                assert!(fitted.1 > 0.0 && fitted.1 <= available);
            }
            let label = format!("3  {}", "e\u{301}".repeat(80));
            let (fitted, measured) = fit_tab_text(&mut fonts, metrics, &label, available);
            assert!(measured <= available);
            let (head, tail) = fitted.split_once('…').expect("long label must elide");
            assert!(head.starts_with("3  ") && head.ends_with('\u{301}'));
            assert!(tail.starts_with('e') && tail.ends_with('\u{301}'));
            assert!(label.starts_with(head) && label.ends_with(tail));
            let exact = fit_tab_text(&mut fonts, metrics, &fitted, f32::INFINITY);
            assert!((exact.1 - measured).abs() < 0.01);

            let identity_width = fit_tab_text(&mut fonts, metrics, "64", f32::INFINITY).1;
            let fitted = fit_tab_text(&mut fonts, metrics, "64  machine_vs_aliens", identity_width);
            assert_eq!(
                fitted.0, "64",
                "keep a fitting identity when the name cannot fit"
            );
            assert!(fitted.1 <= identity_width);
            assert_eq!(
                fit_tab_text(&mut fonts, metrics, "64  eon", 0.0),
                (String::new(), 0.0)
            );
        }
    }

    #[test]
    #[ignore = "requires an isolated native Wayland display and Vulkan renderer"]
    fn long_workspace_labels_stay_on_the_visible_line() {
        use eon_workspace_protocol::v4::{Pane, Snapshot, Tab};
        use winit::{
            application::ApplicationHandler, event::WindowEvent, event_loop::EventLoop,
            platform::wayland::EventLoopBuilderExtWayland, window::WindowId,
        };

        struct Probe(bool);
        impl ApplicationHandler for Probe {
            fn resumed(&mut self, event_loop: &ActiveEventLoop) {
                let window = Arc::new(
                    event_loop
                        .create_window(Window::default_attributes())
                        .unwrap(),
                );
                let mut renderer = pollster::block_on(Renderer::new(
                    window,
                    event_loop,
                    1.0,
                    None,
                    true,
                    FontSetup::new(&FontSettings::default()).unwrap(),
                ))
                .unwrap();
                let snapshot = Snapshot {
                    active_tab: "t2".into(),
                    directory_picker: None,
                    tabs: ["eon", "machines_vs_aliens"]
                        .into_iter()
                        .enumerate()
                        .map(|(index, directory)| Tab {
                            id: format!("t{}", index + 1),
                            directory: format!("/tmp/{directory}").into_bytes(),
                            selected_pane: Some(format!("p{}", index + 1)),
                            panes: vec![Pane {
                                id: format!("p{}", index + 1),
                                session: format!("session-{}", index + 1),
                                endpoint: format!("/tmp/session-{}.sock", index + 1).into_bytes(),
                                live: true,
                            }],
                        })
                        .collect(),
                };
                let workspace = WorkspaceScene::from_snapshot(
                    &snapshot,
                    PhysicalSize::new(900, 600),
                    renderer.metrics(),
                    0.0,
                    0.0,
                    |_, label| renderer.fit_tab_text(label),
                );
                renderer.build_workspace(
                    &workspace,
                    WorkspaceFocus::Terminal,
                    &mut RectangleBatch::new(900, 600),
                );
                let label = &renderer.text[1];
                let line = label.buffer.layout_runs().next().unwrap();
                assert_eq!(
                    line.glyphs.last().unwrap().end,
                    workspace.tabs[1].label().len(),
                    "the directory name wrapped below the visible tab header"
                );
                assert!(workspace.tabs[1].rect.width > workspace.tabs[0].rect.width);
                assert!(line.line_w <= (label.right - label.bound_left) as f32);
                let underscore = line
                    .glyphs
                    .iter()
                    .find(|glyph| &line.text[glyph.start..glyph.end] == "_")
                    .unwrap()
                    .physical((label.left, label.top), 1.0);
                let ink = renderer
                    .swash_cache
                    .get_image(&mut renderer.fonts.font_system, underscore.cache_key)
                    .as_ref()
                    .unwrap();
                let top = line.line_y.round() as i32 + underscore.y - ink.placement.top;
                assert!(ink.placement.height > 0);
                assert!(
                    top >= label.bound_top && top + ink.placement.height as i32 <= label.bottom,
                    "the directory underscore is clipped outside the header"
                );
                renderer.set_hovered_header(Some((WorkspaceFocus::Tabs, "t2".into())));
                renderer.build_tab_tooltip(&workspace, &mut RectangleBatch::new(900, 600));
                let (first, overlay) = renderer.tab_tooltip.unwrap();
                let areas: Vec<_> =
                    text_areas(&renderer.text[..first], renderer.tab_tooltip).collect();
                assert!(
                    areas.iter().all(|area| {
                        let b = area.bounds;
                        b.right <= overlay.left
                            || b.left >= overlay.right
                            || b.bottom <= overlay.top
                            || b.top >= overlay.bottom
                    }),
                    "underlying text must not paint over the hover path"
                );
                assert_eq!(
                    renderer.text[first].buffer.lines[0].text(),
                    "t2  /tmp/machines_vs_aliens"
                );
                renderer.rebuild_if_needed(
                    None,
                    None,
                    0.0,
                    None,
                    WorkspaceFocus::Terminal,
                    &"Sessions unavailable. ".repeat(32),
                    false,
                    "",
                    1,
                );
                assert!(renderer.text[1].buffer.layout_runs().count() > 1);
                self.0 = true;
                event_loop.exit();
            }

            fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
        }
        let event_loop = EventLoop::builder()
            .with_wayland()
            .with_any_thread(true)
            .build()
            .unwrap();
        let mut probe = Probe(false);
        event_loop.run_app(&mut probe).unwrap();
        assert!(probe.0);
    }

    #[test]
    fn device_loss_blocks_surface_recovery_but_destroy_does_not() {
        let lost = AtomicBool::new(false);

        assert!(!record_device_loss(&lost, DeviceLostReason::Destroyed));
        assert_eq!(ensure_device_available(&lost), Ok(()));

        assert!(record_device_loss(&lost, DeviceLostReason::Unknown));
        assert_eq!(
            ensure_device_available(&lost),
            Err(RenderError(DEVICE_LOST.into()))
        );
    }

    #[test]
    fn uncaptured_errors_remain_fatal_until_device_loss() {
        let lost = AtomicBool::new(false);
        let out_of_memory = || wgpu::Error::OutOfMemory {
            source: Box::new(std::io::Error::other("test GPU allocation failure")),
        };

        assert!(
            std::panic::catch_unwind(|| handle_uncaptured_gpu_error(&lost, out_of_memory()))
                .is_err()
        );

        assert!(record_device_loss(&lost, DeviceLostReason::Unknown));
        handle_uncaptured_gpu_error(&lost, out_of_memory());
    }

    #[test]
    fn content_cache_separates_generations() {
        let key = |generation| ContentKey {
            generation,
            scroll_offset: 0.0,
            workspace_focus: WorkspaceFocus::Terminal,
            blink_visible: true,
            preedit: String::new(),
            status: String::new(),
            hyperlink: None,
            hovered_header: None,
        };

        assert_ne!(key(1), key(2));
    }

    #[test]
    fn bounded_preview_rows_cover_multi_row_fractional_offsets() {
        let metrics = CellMetrics {
            width: 10.0,
            height: 20.0,
            font_size: 16.0,
            padding: 5.0,
        };
        let scene = Scene {
            revision: 7,
            columns: 1,
            rows: 2,
            screen: Screen::Primary,
            title: String::new(),
            working_directory: String::new(),
            background: DEFAULT_BACKGROUND,
            foreground: SceneColor::default(),
            cursor: None,
            content: Vec::new(),
        };
        let row = DrawRow {
            wrapped: false,
            wrap_continuation: false,
            kitty_virtual_placeholder: false,
            cells: Vec::new(),
        };
        let viewport = SceneRect {
            left: 0.0,
            top: 0.0,
            width: 20.0,
            height: 50.0,
        };
        let grid = scene_grid(&scene, viewport, metrics);
        assert!(row_intersects_clip(
            grid.top - metrics.height / 2.0,
            metrics.height,
            grid
        ));
        assert!(!row_intersects_clip(grid.bottom(), metrics.height, grid));

        for (direction, offset) in [
            (VerticalDirection::Up, 3.0 * metrics.height - 0.25),
            (VerticalDirection::Down, -3.0 * metrics.height + 0.25),
        ] {
            let preview = ScenePreview::Viewport {
                frame_revision: scene.revision,
                direction,
                edge_reached: false,
                rows: vec![row.clone(); 3],
            };
            let (actual_direction, rows) = preview_rows(Some(&preview)).unwrap();
            assert_eq!(rows.len(), 3);
            let origin = shifted(viewport, offset);
            let first = preview_row_top(&scene, actual_direction, 0, metrics, origin);
            let top = preview_row_top(&scene, actual_direction, 2, metrics, origin);
            assert_eq!((top - first).abs(), 2.0 * metrics.height);
            assert!(
                (SceneRect {
                    left: grid.left,
                    top,
                    width: grid.width,
                    height: metrics.height,
                })
                .intersection(grid)
                .is_some()
            );
        }
    }

    fn plain_style() -> DrawStyle {
        DrawStyle {
            foreground: SceneColor::default(),
            background: DEFAULT_BACKGROUND,
            underline_color: SceneColor::default(),
            bold: false,
            italic: false,
            faint: false,
            blink: false,
            invisible: false,
            strikethrough: false,
            overline: false,
            selected: false,
            background_is_default: true,
            protected: false,
            underline: Underline::None,
        }
    }

    fn grid_buffer(
        font_system: &mut FontSystem,
        metrics: CellMetrics,
        cell_font: CellFont,
        text: &str,
        box_drawing: bool,
        attrs: Attrs<'static>,
    ) -> Buffer {
        let attrs = attrs.letter_spacing(cell_font.letter_spacing);
        let box_attrs = attrs
            .clone()
            .metrics(Metrics::new(cell_font.box_size, metrics.height))
            .letter_spacing(cell_font.box_letter_spacing);
        let braille_attrs = attrs.clone().family(Family::Name(BRAILLE_FAMILY));
        let mut buffer = Buffer::new(font_system, Metrics::new(cell_font.size, metrics.height));
        buffer.set_size(
            Some(metrics.width * text.chars().count() as f32),
            Some(metrics.height),
        );
        buffer.set_wrap(Wrap::None);
        buffer.set_monospace_width(Some(metrics.width));
        set_cell_layer(
            &mut buffer,
            text,
            &cell_text_segments(text),
            &attrs,
            &braille_attrs,
            &box_attrs,
            box_drawing,
        );
        buffer.shape_until_scroll(font_system, false);
        buffer
    }

    #[test]
    fn concealed_cells_draw_no_foreground_elements() {
        let style = DrawStyle {
            blink: true,
            invisible: true,
            strikethrough: true,
            overline: true,
            underline: Underline::Single,
            ..plain_style()
        };
        let scene = Scene {
            revision: 1,
            columns: 1,
            rows: 1,
            screen: Screen::Primary,
            title: String::new(),
            working_directory: String::new(),
            background: DEFAULT_BACKGROUND,
            foreground: SceneColor::default(),
            cursor: None,
            content: vec![DrawRow {
                wrapped: false,
                wrap_continuation: false,
                kitty_virtual_placeholder: false,
                cells: vec![DrawCell {
                    width: CellWidth::Narrow,
                    text: "secret".into(),
                    hyperlink: String::new(),
                    style,
                }],
            }],
        };
        let mut rectangles = RectangleBatch::new(100, 100);

        build_scene_rectangles(
            &mut rectangles,
            &scene,
            true,
            CellMetrics::for_scale(1.0),
            SceneRect {
                left: 0.0,
                top: 0.0,
                width: 100.0,
                height: 100.0,
            },
            true,
        );

        assert!(rectangles.bytes.is_empty());
        assert!(scene.glyph_runs().is_empty());
        assert!(!scene.has_blinking_content());
    }

    #[test]
    fn full_block_cells_use_exact_cell_rectangles() {
        let colors = [
            SceneColor { r: 255, g: 0, b: 0 },
            SceneColor { r: 0, g: 255, b: 0 },
            SceneColor { r: 0, g: 0, b: 255 },
            SceneColor {
                r: 255,
                g: 0,
                b: 255,
            },
        ];
        let scene = Scene {
            revision: 1,
            columns: 2,
            rows: 2,
            screen: Screen::Primary,
            title: String::new(),
            working_directory: String::new(),
            background: DEFAULT_BACKGROUND,
            foreground: SceneColor::default(),
            cursor: None,
            content: colors
                .chunks_exact(2)
                .map(|row| DrawRow {
                    wrapped: false,
                    wrap_continuation: false,
                    kitty_virtual_placeholder: false,
                    cells: row
                        .iter()
                        .map(|foreground| DrawCell {
                            width: CellWidth::Narrow,
                            text: "█".into(),
                            hyperlink: String::new(),
                            style: DrawStyle {
                                foreground: *foreground,
                                faint: *foreground == colors[2],
                                ..plain_style()
                            },
                        })
                        .collect(),
                })
                .collect(),
        };

        let glyph_runs = scene.glyph_runs();
        assert_eq!(glyph_runs.len(), 4);
        assert!(glyph_runs.iter().all(is_full_block_run));
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let metrics = CellMetrics::for_scale(scale);
            let viewport = SceneRect {
                left: 7.0,
                top: 11.0,
                width: 200.0,
                height: 200.0,
            };
            let mut actual = RectangleBatch::new(300, 300);
            build_scene_rectangles(&mut actual, &scene, true, metrics, viewport, true);
            let mut expected = RectangleBatch::new(300, 300);
            for (index, color) in colors.into_iter().enumerate() {
                expected.push(
                    viewport.left + metrics.padding + (index % 2) as f32 * metrics.width,
                    viewport.top + metrics.padding + (index / 2) as f32 * metrics.height,
                    metrics.width,
                    metrics.height,
                    color,
                    if index == 2 { 150.0 / 255.0 } else { 1.0 },
                );
            }
            assert_eq!(actual.bytes, expected.bytes, "scale {scale}");
        }
    }

    #[test]
    fn wide_cell_cursors_cover_the_complete_glyph() {
        let style = plain_style();
        let mut scene = Scene {
            revision: 1,
            columns: 3,
            rows: 1,
            screen: Screen::Primary,
            title: String::new(),
            working_directory: String::new(),
            background: DEFAULT_BACKGROUND,
            foreground: SceneColor::default(),
            cursor: None,
            content: vec![DrawRow {
                wrapped: false,
                wrap_continuation: false,
                kitty_virtual_placeholder: false,
                cells: [CellWidth::Narrow, CellWidth::Wide, CellWidth::SpacerTail]
                    .into_iter()
                    .map(|width| DrawCell {
                        width,
                        text: String::new(),
                        hyperlink: String::new(),
                        style,
                    })
                    .collect(),
            }],
        };
        let metrics = CellMetrics::for_scale(1.0);
        let cursor = DrawCursor {
            visible: true,
            blinking: false,
            password_input: false,
            shape: CursorShape::Block,
            column: 0,
            row: 0,
            at_wide_tail: false,
            color: SceneColor::default(),
        };
        let mut draw_cursor = |cursor| {
            scene.cursor = Some(cursor);
            let mut rectangles = RectangleBatch::new(100, 100);
            build_scene_rectangles(
                &mut rectangles,
                &scene,
                true,
                metrics,
                SceneRect {
                    left: 0.0,
                    top: 0.0,
                    width: 100.0,
                    height: 100.0,
                },
                true,
            );
            rectangles.bytes
        };
        let expected = |column: f32, columns: f32| {
            let mut rectangles = RectangleBatch::new(100, 100);
            rectangles.push(
                metrics.padding + column * metrics.width,
                metrics.padding,
                columns * metrics.width,
                metrics.height,
                cursor.color,
                0.55,
            );
            rectangles.bytes
        };

        assert_eq!(draw_cursor(cursor), expected(0.0, 1.0));
        assert_eq!(
            draw_cursor(DrawCursor {
                column: 1,
                ..cursor
            }),
            expected(1.0, 2.0)
        );
        assert_eq!(
            draw_cursor(DrawCursor {
                column: 2,
                at_wide_tail: true,
                ..cursor
            }),
            expected(1.0, 2.0)
        );
    }

    #[test]
    fn cursor_spring_snaps_then_distinguishes_short_long_and_scaled_motion() {
        let origin = SceneRect {
            left: 10.0,
            top: 20.0,
            width: 10.0,
            height: 18.0,
        };
        let short = SceneRect {
            left: 20.0,
            ..origin
        };
        let long = SceneRect {
            left: 60.0,
            top: 74.0,
            ..origin
        };
        let viewport = SceneRect {
            left: 0.0,
            top: 0.0,
            width: 200.0,
            height: 200.0,
        };
        let cell_width = origin.width;

        let mut animation = CursorAnimation::default();
        assert!(!animation.update(origin, Some("pane-1"), viewport, cell_width, 0.0, 1.0));
        assert_eq!(animation.corners(), rect_corners(origin));
        assert!(animation.update(short, Some("pane-1"), viewport, cell_width, 0.0, 1.0));
        assert!(!animation.update(short, Some("pane-1"), viewport, cell_width, 0.05, 1.0));
        assert_eq!(animation.corners(), rect_corners(short));

        animation.reset();
        assert!(!animation.update(origin, None, viewport, cell_width, 0.0, 1.0));
        assert!(animation.update(long, None, viewport, cell_width, 0.0, 1.0));
        assert!(animation.update(long, None, viewport, cell_width, 0.05, 1.0));

        let wide_origin = SceneRect {
            width: 20.0,
            ..origin
        };
        let mut wide_three_cells = SceneRect {
            left: 40.0,
            ..wide_origin
        };
        let mut wide_animation = CursorAnimation::default();
        assert!(!wide_animation.update(wide_origin, None, viewport, cell_width, 0.0, 1.0));
        assert!(wide_animation.update(wide_three_cells, None, viewport, cell_width, 0.0, 1.0));
        assert!(wide_animation.update(wide_three_cells, None, viewport, cell_width, 0.05, 1.0));
        let before_retarget = wide_animation.corners()[0].x;
        wide_three_cells.left = 80.0;
        assert!(wide_animation.update(wide_three_cells, None, viewport, cell_width, 0.0, 1.0));
        assert!((wide_animation.corners()[0].x - before_retarget).abs() < CURSOR_SETTLED);

        let started = |scale| {
            let mut animation = CursorAnimation::default();
            assert!(!animation.update(origin, None, viewport, cell_width, 0.0, scale));
            assert!(animation.update(long, None, viewport, cell_width, 0.0, scale));
            animation
        };
        let mut fast = started(0.25);
        let mut slow = started(4.0);
        assert!(!fast.update(long, None, viewport, cell_width, 0.05, 0.25));
        assert!(slow.update(long, None, viewport, cell_width, 0.05, 4.0));

        let mut bounded = started(4.0);
        let mut huge_delta = bounded.clone();
        assert_eq!(
            bounded.update(long, None, viewport, cell_width, 0.1, 4.0),
            huge_delta.update(long, None, viewport, cell_width, 10.0, 4.0)
        );
        assert_eq!(bounded.corners(), huge_delta.corners());

        for _ in 0..120 {
            if !slow.update(long, None, viewport, cell_width, 1.0 / 60.0, 4.0) {
                break;
            }
        }
        assert!(!slow.is_active());
        assert_eq!(slow.corners(), rect_corners(long));

        let moved = SceneRect { left: 80.0, ..long };
        assert!(!slow.update(long, Some("pane-1"), viewport, cell_width, 0.0, 1.0));
        assert!(slow.update(moved, Some("pane-1"), viewport, cell_width, 0.0, 1.0));
        assert!(!slow.update(moved, Some("pane-2"), viewport, cell_width, 0.0, 1.0));
        assert_eq!(slow.corners(), rect_corners(moved));
        assert!(!slow.update(
            moved,
            Some("pane-2"),
            SceneRect {
                top: 10.0,
                ..viewport
            },
            cell_width,
            0.0,
            1.0,
        ));
    }

    #[test]
    fn tail_cursor_uses_one_color_and_resets_when_hidden_or_blinking_off() {
        let style = plain_style();
        let mut scene = Scene {
            revision: 1,
            columns: 4,
            rows: 1,
            screen: Screen::Primary,
            title: String::new(),
            working_directory: String::new(),
            background: DEFAULT_BACKGROUND,
            foreground: SceneColor::default(),
            cursor: Some(DrawCursor {
                visible: true,
                blinking: false,
                password_input: false,
                shape: CursorShape::Block,
                column: 0,
                row: 0,
                at_wide_tail: false,
                color: SceneColor { r: 1, g: 2, b: 3 },
            }),
            content: vec![DrawRow {
                wrapped: false,
                wrap_continuation: false,
                kitty_virtual_placeholder: false,
                cells: [
                    CellWidth::Narrow,
                    CellWidth::Wide,
                    CellWidth::SpacerTail,
                    CellWidth::Narrow,
                ]
                .into_iter()
                .map(|width| DrawCell {
                    width,
                    text: String::new(),
                    hyperlink: String::new(),
                    style,
                })
                .collect(),
            }],
        };
        let viewport = SceneRect {
            left: 0.0,
            top: 0.0,
            width: 100.0,
            height: 100.0,
        };
        let metrics = CellMetrics::for_scale(1.0);
        let trail = SceneColor {
            r: 0x12,
            g: 0xab,
            b: 0xcf,
        };
        let mut animation = CursorAnimation::default();
        let mut first = RectangleBatch::new(100, 100);
        assert!(!build_tail_cursor(
            &mut first,
            &mut animation,
            &scene,
            true,
            metrics,
            viewport,
            None,
            trail,
            1.0,
            0.0,
        ));
        assert_eq!(
            first.bytes.len(),
            VERTEX_SIZE as usize * VERTICES_PER_QUAD as usize
        );

        scene.cursor.as_mut().unwrap().column = 2;
        scene.cursor.as_mut().unwrap().at_wide_tail = true;
        let mut moving = RectangleBatch::new(100, 100);
        assert!(build_tail_cursor(
            &mut moving,
            &mut animation,
            &scene,
            true,
            metrics,
            viewport,
            None,
            trail,
            1.0,
            0.0,
        ));
        assert_eq!(
            moving.bytes.len(),
            VERTEX_SIZE as usize * VERTICES_PER_QUAD as usize * 2
        );
        for vertex in moving.bytes[..VERTEX_SIZE as usize * VERTICES_PER_QUAD as usize]
            .chunks_exact(VERTEX_SIZE as usize)
        {
            assert_eq!(
                f32::from_ne_bytes(vertex[8..12].try_into().unwrap()),
                0x12 as f32 / 255.0
            );
            assert_eq!(
                f32::from_ne_bytes(vertex[12..16].try_into().unwrap()),
                0xab as f32 / 255.0
            );
            assert_eq!(
                f32::from_ne_bytes(vertex[16..20].try_into().unwrap()),
                0xcf as f32 / 255.0
            );
        }
        let mut tick = RectangleBatch::new(100, 100);
        assert!(build_tail_cursor(
            &mut tick,
            &mut animation,
            &scene,
            true,
            metrics,
            viewport,
            None,
            trail,
            1.0,
            0.016,
        ));

        scene.cursor.as_mut().unwrap().blinking = true;
        let mut hidden = RectangleBatch::new(100, 100);
        assert!(!build_tail_cursor(
            &mut hidden,
            &mut animation,
            &scene,
            false,
            metrics,
            viewport,
            None,
            trail,
            1.0,
            0.01,
        ));
        assert!(hidden.bytes.is_empty());
        assert!(!animation.is_active());
    }

    #[test]
    fn dynamic_cursor_keeps_later_static_overlays_on_top() {
        assert_eq!(cursor_vertex_boundary(18, 12, true), 12);
        assert_eq!(cursor_vertex_boundary(18, 12, false), 18);
        assert_eq!(cursor_vertex_boundary(18, 24, true), 18);
    }

    #[test]
    fn trail_quad_is_two_clipped_finite_triangles_in_stable_order() {
        let mut rectangles = RectangleBatch::new(100, 100);
        rectangles.clip = Some(SceneRect {
            left: 10.0,
            top: 20.0,
            width: 50.0,
            height: 40.0,
        });
        let points = [
            CursorPoint { x: -5.0, y: 10.0 },
            CursorPoint { x: 70.0, y: 15.0 },
            CursorPoint { x: 65.0, y: 70.0 },
            CursorPoint { x: 5.0, y: 65.0 },
        ];
        rectangles.push_quad(points, SceneColor { r: 1, g: 2, b: 3 }, 1.0);

        assert_eq!(
            rectangles.bytes.len(),
            VERTEX_SIZE as usize * VERTICES_PER_QUAD as usize
        );
        let positions = rectangles
            .bytes
            .chunks_exact(VERTEX_SIZE as usize)
            .map(|vertex| {
                (
                    f32::from_ne_bytes(vertex[..4].try_into().unwrap()),
                    f32::from_ne_bytes(vertex[4..8].try_into().unwrap()),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            positions,
            [
                points[0], points[1], points[2], points[0], points[2], points[3]
            ]
            .map(|point| {
                let x = point.x.clamp(10.0, 60.0) / 100.0 * 2.0 - 1.0;
                let y = 1.0 - point.y.clamp(20.0, 60.0) / 100.0 * 2.0;
                (x, y)
            })
        );
        assert!(
            positions
                .iter()
                .all(|(x, y)| x.is_finite() && y.is_finite())
        );

        rectangles.push_quad(
            [CursorPoint {
                x: f32::NAN,
                y: 0.0,
            }; 4],
            SceneColor::default(),
            1.0,
        );
        assert_eq!(
            rectangles.bytes.len(),
            VERTEX_SIZE as usize * VERTICES_PER_QUAD as usize
        );
    }

    #[test]
    fn rounded_outline_keeps_corners_and_interior_clear_when_clipped() {
        let bounds = SceneRect {
            left: 10.0,
            top: 20.0,
            width: 80.0,
            height: 60.0,
        };
        for radius in [0.0, 12.0, 100.0] {
            for clip in [
                bounds,
                SceneRect {
                    top: 30.0,
                    height: 40.0,
                    ..bounds
                },
            ] {
                let mut batch = RectangleBatch::new(100, 100);
                batch.clip = Some(clip);
                batch.push_rounded_outline(bounds, radius, SceneColor::default());
                let quads: Vec<_> = batch
                    .bytes
                    .chunks_exact(VERTEX_SIZE as usize * 6)
                    .map(|quad| {
                        let value = |vertex: usize, component: usize| {
                            let offset = vertex * VERTEX_SIZE as usize + component * 4;
                            f32::from_ne_bytes(quad[offset..offset + 4].try_into().unwrap())
                        };
                        let rect = SceneRect {
                            left: (value(0, 0) + 1.0) * 50.0,
                            top: (1.0 - value(0, 1)) * 50.0,
                            width: (value(2, 0) - value(0, 0)) * 50.0,
                            height: (value(0, 1) - value(2, 1)) * 50.0,
                        };
                        let alpha = value(0, 5);
                        assert!((0.0..=1.0).contains(&alpha));
                        assert!(
                            rect.left >= clip.left - 0.001 && rect.right() <= clip.right() + 0.001
                        );
                        assert!(
                            rect.top >= clip.top - 0.001 && rect.bottom() <= clip.bottom() + 0.001
                        );
                        (rect, alpha)
                    })
                    .collect();
                let painted = |x, y| {
                    quads
                        .iter()
                        .any(|(rect, alpha)| *alpha > 0.0 && rect.contains(x, y))
                };
                assert!(!painted(50.0, 50.0), "outline must not fill the terminal");
                assert!(painted(10.5, 50.0), "side stroke is missing");
                if clip == bounds {
                    assert!(painted(50.0, 20.5), "top stroke is missing");
                    if radius > 1.0 {
                        assert!(!painted(10.5, 20.5), "corner is square");
                    }
                } else {
                    assert!(
                        !painted(50.0, 30.5),
                        "clipping must not create a new top border"
                    );
                }
            }
        }
    }

    #[test]
    fn rectangle_encoding_is_row_major_and_bounded() {
        let mut rectangles = RectangleBatch::new(100, 100);
        rectangles.push(0.0, 0.0, 10.0, 20.0, SceneColor { r: 1, g: 2, b: 3 }, 1.0);
        assert_eq!(
            rectangles.bytes.len(),
            VERTEX_SIZE as usize * VERTICES_PER_QUAD as usize
        );
        assert_eq!(&rectangles.bytes[..4], &(-1.0_f32).to_ne_bytes());
        assert_eq!(&rectangles.bytes[4..8], &1.0_f32.to_ne_bytes());
    }

    #[test]
    fn default_font_matches_nova_at_supported_scales() {
        let mut font_system = FontSystem::new();
        for (scale, width, height, font_size, padding) in [
            (1.0, 10.0, 18.0, 16.0, 12.0),
            (1.25, 13.0, 23.0, 20.0, 15.0),
            (1.5, 15.0, 27.0, 24.0, 18.0),
            (2.0, 20.0, 36.0, 32.0, 24.0),
        ] {
            let metrics = CellMetrics::for_scale(scale);
            assert_eq!(
                (
                    metrics.width,
                    metrics.height,
                    metrics.font_size,
                    metrics.padding
                ),
                (width, height, font_size, padding)
            );
            assert_eq!(
                fitted_cell_font(&mut font_system, metrics, None, false).size,
                font_size
            );
        }
    }

    #[test]
    fn background_opacity_selects_only_proved_surface_modes() {
        assert_eq!(
            surface_alpha_mode(1.0, &[CompositeAlphaMode::Inherit]).unwrap(),
            CompositeAlphaMode::Auto
        );
        assert_eq!(
            surface_alpha_mode(
                0.88,
                &[
                    CompositeAlphaMode::Opaque,
                    CompositeAlphaMode::PreMultiplied,
                ],
            )
            .unwrap(),
            CompositeAlphaMode::PreMultiplied
        );
        assert!(surface_alpha_mode(0.0, &[CompositeAlphaMode::Opaque]).is_err());
        assert!(surface_alpha_mode(0.5, &[CompositeAlphaMode::PostMultiplied]).is_err());
    }

    #[test]
    fn background_opacity_premultiplies_clear_colors() {
        let source = SceneColor {
            r: 128,
            g: 0,
            b: 255,
        };
        let opaque = clear_color(source, 1.0, true);
        let translucent = clear_color(source, 0.5, true);
        let encoded = clear_color(source, 1.0, false);

        assert!((opaque.r - 0.215_860_5).abs() < 0.000_001);
        assert_eq!(opaque.a, 1.0);
        assert!((translucent.r - 0.107_930_25).abs() < 0.000_001);
        assert_eq!(translucent.g, 0.0);
        assert_eq!(translucent.b, 0.5);
        assert_eq!(translucent.a, 0.5);
        assert_eq!(encoded.r, 128.0 / 255.0);
    }

    #[test]
    fn background_opacity_skips_only_default_cell_backgrounds() {
        let mut scene = Scene {
            revision: 1,
            columns: 4,
            rows: 1,
            screen: Screen::Primary,
            title: String::new(),
            working_directory: String::new(),
            background: DEFAULT_BACKGROUND,
            foreground: SceneColor::default(),
            cursor: None,
            content: vec![DrawRow {
                wrapped: false,
                wrap_continuation: false,
                kitty_virtual_placeholder: false,
                cells: (0..4)
                    .map(|_| DrawCell {
                        width: CellWidth::Narrow,
                        text: String::new(),
                        hyperlink: String::new(),
                        style: plain_style(),
                    })
                    .collect(),
            }],
        };
        scene.content[0].cells[1].style.background_is_default = false;
        scene.content[0].cells[2].style.background_is_default = false;
        scene.content[0].cells[3].style.background_is_default = false;

        let metrics = CellMetrics::for_scale(1.0);
        let viewport = SceneRect {
            left: 0.0,
            top: 0.0,
            width: 100.0,
            height: 100.0,
        };
        let mut actual = RectangleBatch::new(100, 100);
        build_scene_rectangles(&mut actual, &scene, true, metrics, viewport, true);
        let mut expected = RectangleBatch::new(100, 100);
        expected.push(
            metrics.padding + metrics.width,
            metrics.padding,
            metrics.width * 3.0,
            metrics.height,
            DEFAULT_BACKGROUND,
            1.0,
        );

        assert_eq!(actual.bytes, expected.bytes);
    }

    #[test]
    fn shaped_preedit_follows_emitted_glyphs_without_charging_combining_marks() {
        let mut fonts = FontSetup::new(&FontSettings {
            family: Some("DejaVu Sans Mono".into()),
            size: 20.0,
            line_height: 1.5,
            ..Default::default()
        })
        .unwrap();
        let metrics = fonts.metrics(1.25);
        let cell_font = fitted_cell_font(&mut fonts.font_system, metrics, fonts.family, true);
        let mut font_system = fonts.font_system;
        let mut text_width = |text: &str| {
            let mut buffer = Buffer::new(
                &mut font_system,
                Metrics::new(cell_font.size, metrics.height),
            );
            buffer.set_size(Some(100.0), Some(metrics.height));
            buffer.set_wrap(Wrap::None);
            buffer.set_monospace_width(Some(metrics.width));
            buffer.set_text(text, &cell_font.attrs(), shaping(text), None);
            buffer.shape_until_scroll(&mut font_system, false);
            let glyphs = buffer
                .layout_runs()
                .next()
                .expect("the preedit should shape")
                .glyphs;
            let (left, right) = glyphs.iter().fold(
                (f32::INFINITY, f32::NEG_INFINITY),
                |(left, right), glyph| (left.min(glyph.x), right.max(glyph.x + glyph.w)),
            );
            let (left_offset, width) = shaped_preedit_placement(&buffer);
            (width, right - left, left + left_offset)
        };

        let widths = ["e", "eee", "e\u{301}", "אבג"].map(|text| {
            let (width, emitted_width, left) = text_width(text);
            assert!(
                (width - emitted_width).abs() < 0.01,
                "{text:?} measured {width} but emitted {emitted_width}"
            );
            assert!(left.abs() < 0.01, "{text:?} starts at {left}");
            width
        });
        assert!((widths[0] - widths[2]).abs() < 0.01);
    }

    #[test]
    fn text_runs_follow_the_cell_grid_and_box_borders_connect() {
        let mut font_system = FontSystem::new();
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let metrics = CellMetrics::for_scale(scale);
            let cell_font = fitted_cell_font(&mut font_system, metrics, None, false);
            for (text, box_drawing, attrs) in [
                ("narrow text", false, cell_font.attrs()),
                (
                    "┌──┬──┐",
                    true,
                    cell_font
                        .attrs()
                        .weight(Weight::BOLD)
                        .color(Color::rgb(80, 200, 160)),
                ),
                ("italic text", false, cell_font.attrs().style(Style::Italic)),
            ] {
                let buffer = grid_buffer(
                    &mut font_system,
                    metrics,
                    cell_font,
                    text,
                    box_drawing,
                    attrs,
                );
                let glyphs = buffer
                    .layout_runs()
                    .next()
                    .expect("the terminal run should shape")
                    .glyphs;
                assert_eq!(glyphs.len(), text.chars().count());
                for (column, glyph) in glyphs.iter().enumerate() {
                    assert!((glyph.x - column as f32 * metrics.width).abs() < 0.01);
                    assert!(
                        (glyph.w - metrics.width).abs() < 0.01,
                        "{text:?} column {column} at scale {scale} has width {} instead of {}",
                        glyph.w,
                        metrics.width
                    );
                }
            }

            let mixed = "e\u{301}│abc│";
            let segments = cell_text_segments(mixed);
            assert_eq!(
                segments,
                [
                    ("e\u{301}", CellTextKind::Text),
                    ("│", CellTextKind::Box),
                    ("abc", CellTextKind::Text),
                    ("│", CellTextKind::Box)
                ]
            );
            let layer_widths = [false, true].map(|box_drawing| {
                let buffer = grid_buffer(
                    &mut font_system,
                    metrics,
                    cell_font,
                    mixed,
                    box_drawing,
                    cell_font.attrs(),
                );
                shaped_preedit_placement(&buffer).1
            });
            for layer_width in layer_widths {
                assert!(
                    (layer_width - metrics.width * 6.0).abs() < 0.01,
                    "mixed combining and box text at scale {scale} occupies {layer_width} instead of {}",
                    metrics.width * 6.0
                );
            }
            let fallback_widths = [false, true].map(|box_drawing| {
                let buffer = grid_buffer(
                    &mut font_system,
                    metrics,
                    cell_font,
                    "אבג│",
                    box_drawing,
                    cell_font.attrs(),
                );
                shaped_preedit_placement(&buffer).1
            });
            assert!(
                (fallback_widths[0] - fallback_widths[1]).abs() < 0.01,
                "mixed fallback and box layers diverge at scale {scale}: {fallback_widths:?}"
            );

            let buffer = grid_buffer(
                &mut font_system,
                metrics,
                cell_font,
                "│",
                true,
                cell_font.attrs(),
            );
            let run = buffer
                .layout_runs()
                .next()
                .expect("the vertical border should shape");
            let glyph = run
                .glyphs
                .first()
                .expect("the vertical border should exist");
            let physical = glyph.physical((0.0, 0.0), 1.0);
            let image = SwashCache::new()
                .get_image_uncached(&mut font_system, physical.cache_key)
                .expect("the vertical border glyph should rasterize");
            let top = run.line_y.round() as i32 + physical.y - image.placement.top;
            let bottom = top + image.placement.height as i32;
            assert!(
                top <= 0 && bottom >= metrics.height as i32,
                "vertical border at scale {scale} occupies {top}..{bottom} in a {} px row",
                metrics.height
            );
        }

        let metrics = CellMetrics::for_scale(1.0);
        let cell_font = fitted_cell_font(&mut font_system, metrics, None, false);
        let buffer = grid_buffer(
            &mut font_system,
            metrics,
            cell_font,
            "─────",
            true,
            cell_font.attrs(),
        );
        let glyphs = buffer
            .layout_runs()
            .next()
            .expect("the table border should shape")
            .glyphs;
        let mut swash_cache = SwashCache::new();
        let ink = glyphs
            .iter()
            .map(|glyph| {
                let physical = glyph.physical((0.0, 0.0), 1.0);
                let image = swash_cache
                    .get_image_uncached(&mut font_system, physical.cache_key)
                    .expect("the table border glyph should rasterize");
                let left = physical.x + image.placement.left;
                (left, left + image.placement.width as i32)
            })
            .collect::<Vec<_>>();
        for pair in ink.windows(2) {
            assert!(
                pair[0].1 >= pair[1].0,
                "adjacent table-border cells must not expose a gap: {pair:?}"
            );
        }
    }

    #[test]
    fn braille_progress_uses_distinct_packaged_glyphs_on_the_cell_grid() {
        const SPINNER: &str = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏";
        assert_eq!(
            cell_text_segments(&format!("{SPINNER}│X")),
            [
                (SPINNER, CellTextKind::Braille),
                ("│", CellTextKind::Box),
                ("X", CellTextKind::Text),
            ]
        );

        let mut font_system = FontSystem::new();
        let mut swash_cache = SwashCache::new();
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let metrics = CellMetrics::for_scale(scale);
            let cell_font = fitted_cell_font(&mut font_system, metrics, None, false);
            for frame in SPINNER.chars() {
                let text = format!("{frame}│X");
                let buffer = grid_buffer(
                    &mut font_system,
                    metrics,
                    cell_font,
                    &text,
                    false,
                    cell_font.attrs(),
                );
                let run = buffer
                    .layout_runs()
                    .next()
                    .expect("the Braille frame should shape");
                assert_eq!(run.glyphs.len(), 3);
                let glyph = &run.glyphs[0];
                let face = font_system
                    .db()
                    .face(glyph.font_id)
                    .expect("the Braille face should remain loaded");
                assert!(
                    face.families.iter().any(|(name, _)| name == BRAILLE_FAMILY),
                    "{frame} at scale {scale} used {}",
                    face.post_script_name
                );
                let physical = glyph.physical((0.0, 0.0), 1.0);
                let image = swash_cache
                    .get_image_uncached(&mut font_system, physical.cache_key)
                    .expect("the Braille frame should rasterize");
                let left = physical.x + image.placement.left;
                let right = left + image.placement.width as i32;
                let top = run.line_y.round() as i32 + physical.y
                    - image.placement.top
                    - cell_font.top_offset as i32;
                let bottom = top + image.placement.height as i32;
                assert!(
                    left >= 0
                        && right <= metrics.width as i32
                        && top >= 0
                        && bottom <= metrics.height as i32,
                    "{frame} at scale {scale} occupies {left}..{right} by {top}..{bottom} in a {} by {} cell",
                    metrics.width,
                    metrics.height
                );
            }
        }
    }

    #[test]
    fn descender_rasters_fit_the_cell_row_at_supported_scales() {
        let mut font_system = FontSystem::new();
        let mut swash_cache = SwashCache::new();
        let mut failures = Vec::new();
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let metrics = CellMetrics::for_scale(scale);
            let cell_font = fitted_cell_font(&mut font_system, metrics, None, false);
            for (style, attrs) in [
                ("regular", cell_font.attrs()),
                ("bold", cell_font.attrs().weight(Weight::BOLD)),
                ("italic", cell_font.attrs().style(Style::Italic)),
            ] {
                let text = "gjpqy";
                let mut buffer = Buffer::new(
                    &mut font_system,
                    Metrics::new(cell_font.size, metrics.height),
                );
                buffer.set_size(
                    Some(metrics.width * text.chars().count() as f32),
                    Some(metrics.height),
                );
                buffer.set_wrap(Wrap::None);
                buffer.set_monospace_width(Some(metrics.width));
                buffer.set_text(text, &attrs, shaping(text), None);
                buffer.shape_until_scroll(&mut font_system, false);
                let run = buffer
                    .layout_runs()
                    .next()
                    .expect("the descender corpus should shape");
                assert_eq!(run.glyphs.len(), text.chars().count());
                for (character, glyph) in text.chars().zip(run.glyphs) {
                    let physical = glyph.physical((0.0, 0.0), 1.0);
                    let image = swash_cache
                        .get_image_uncached(&mut font_system, physical.cache_key)
                        .expect("the descender glyph should rasterize");
                    let top = run.line_y.round() as i32 + physical.y
                        - image.placement.top
                        - cell_font.top_offset as i32;
                    let bottom = top + image.placement.height as i32;
                    if top < 0 || bottom > metrics.height as i32 {
                        failures.push(format!(
                            "{style} {character:?} at scale {scale} occupies {top}..{bottom} in a {} px row",
                            metrics.height
                        ));
                    }
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn nerd_font_symbols_use_the_packaged_face_and_bounded_blank_space() {
        assert!(is_nerd_font_symbol("\u{e000}"));
        assert!(is_nerd_font_symbol("\u{f8ff}"));
        assert!(is_nerd_font_symbol("\u{f0000}"));
        assert!(is_nerd_font_symbol("\u{f2fff}"));
        assert!(!is_nerd_font_symbol("\u{f900}"));
        assert!(!is_nerd_font_symbol("\u{f3000}"));

        let mut font_system = FontSystem::new();
        let mut swash_cache = SwashCache::new();
        let metrics = CellMetrics::for_scale(1.0);
        let cell_font = fitted_cell_font(&mut font_system, metrics, None, false);
        let mut observed_overhang = false;
        for symbol in ["\u{e5ff}", "\u{f015}", "\u{f15b}", "\u{f489}", "\u{f0868}"] {
            let geometry = [false, true].map(|bold| {
                let attrs = cell_text_attrs(
                    cell_font,
                    symbol,
                    DrawStyle {
                        bold,
                        ..plain_style()
                    },
                );
                assert_eq!(attrs.weight, Weight::NORMAL);
                let buffer =
                    grid_buffer(&mut font_system, metrics, cell_font, symbol, false, attrs);
                let run = buffer
                    .layout_runs()
                    .next()
                    .expect("the Nerd Font symbol should shape");
                let glyph = run
                    .glyphs
                    .first()
                    .expect("the Nerd Font symbol should emit one glyph");
                let face = font_system
                    .db()
                    .face(glyph.font_id)
                    .expect("the selected symbol face should remain loaded");
                assert!(
                    face.families
                        .iter()
                        .any(|(family, _)| family == NERD_FONT_FAMILY),
                    "{symbol:?} used {}",
                    face.post_script_name
                );
                let physical = glyph.physical((0.0, 0.0), 1.0);
                let image = swash_cache
                    .get_image_uncached(&mut font_system, physical.cache_key)
                    .expect("the Nerd Font symbol should rasterize");
                let left = physical.x + image.placement.left;
                let right = left + image.placement.width as i32;
                observed_overhang |= right > metrics.width as i32;
                assert!(
                    left >= 0 && right <= (metrics.width * 2.0) as i32,
                    "{symbol:?} occupies {left}..{right} with one blank cell available"
                );
                (
                    glyph.font_id,
                    glyph.font_size.to_bits(),
                    glyph.w.to_bits(),
                    left,
                    right,
                )
            });
            assert_eq!(geometry[0], geometry[1]);
        }
        assert!(
            observed_overhang,
            "the corpus must exercise the clipping defect"
        );

        let symbol = GlyphRun {
            column: 0,
            row: 0,
            columns: 1,
            text: "\u{f015}".into(),
            style: plain_style(),
        };
        let label = GlyphRun {
            column: 2,
            text: "x".into(),
            ..symbol.clone()
        };
        assert_eq!(cell_ink_right(&symbol, Some(&label), 4), 2);
        assert_eq!(
            cell_ink_right(
                &symbol,
                Some(&GlyphRun {
                    column: 1,
                    ..label.clone()
                }),
                4,
            ),
            1
        );
        assert_eq!(cell_ink_right(&label, None, 4), 3);
    }

    #[test]
    fn ascii_uses_basic_shaping_without_weakening_unicode() {
        assert_eq!(shaping("plain ASCII"), Shaping::Basic);
        assert_eq!(shaping("e\u{301}"), Shaping::Advanced);
    }
}
