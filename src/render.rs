use crate::{Color as SceneColor, DrawStyle, Scene, SceneRect, WorkspaceFocus, WorkspaceScene};
use glyphon::{
    Attrs, Buffer, Cache, Color, ColorMode, Family, FontSystem, Metrics, Resolution, Shaping,
    Style, SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport, Weight, Wrap,
};
use orbit_protocol::{CellWidth, CursorShape, Underline};
use std::{error::Error, fmt, sync::Arc};
use wgpu::{
    BlendState, BufferDescriptor, BufferUsages, ColorTargetState, ColorWrites,
    CommandEncoderDescriptor, CompositeAlphaMode, CurrentSurfaceTexture, DeviceDescriptor,
    FragmentState, Instance, InstanceDescriptor, LoadOp, MultisampleState, Operations,
    PipelineCompilationOptions, PipelineLayoutDescriptor, PresentMode, PrimitiveState,
    RenderPassColorAttachment, RenderPassDescriptor, RenderPipelineDescriptor,
    RequestAdapterOptions, ShaderModuleDescriptor, ShaderSource, StoreOp, Surface,
    SurfaceConfiguration, TextureViewDescriptor, VertexAttribute, VertexBufferLayout, VertexFormat,
    VertexState, VertexStepMode,
};
use winit::{dpi::PhysicalSize, event_loop::ActiveEventLoop, window::Window};

const VERTEX_SIZE: u64 = 24;
#[cfg(test)]
const VERTICES_PER_QUAD: u32 = 6;
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

impl CellMetrics {
    #[must_use]
    pub fn for_scale(scale_factor: f64) -> Self {
        let scale = scale_factor as f32;
        Self {
            width: (9.0 * scale).round().max(1.0),
            height: (18.0 * scale).round().max(1.0),
            font_size: (14.0 * scale).max(1.0),
            padding: (12.0 * scale).round(),
        }
    }
}

/// Result of one bounded surface presentation attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentOutcome {
    Presented,
    Deferred,
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

#[derive(Clone, Debug, PartialEq)]
struct ContentKey {
    revision: Option<u64>,
    workspace: Option<WorkspaceScene>,
    workspace_focus: WorkspaceFocus,
    blink_visible: bool,
    preedit: String,
    status: String,
}

/// One wgpu surface and one glyphon text owner for the native window.
pub struct Renderer {
    instance: Instance,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: Surface<'static>,
    config: SurfaceConfiguration,
    rectangle_pipeline: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
    vertex_capacity: u64,
    vertex_count: u32,
    font_system: FontSystem,
    swash_cache: SwashCache,
    viewport: Viewport,
    atlas: TextAtlas,
    text_renderer: TextRenderer,
    text: Vec<PlacedText>,
    content_key: Option<ContentKey>,
    clear: wgpu::Color,
    metrics: CellMetrics,
    cell_font_size: f32,
    window: Arc<Window>,
}

impl Renderer {
    pub async fn new(
        window: Arc<Window>,
        event_loop: &ActiveEventLoop,
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
        let mut config = surface
            .get_default_config(&adapter, size.width, size.height)
            .ok_or_else(|| RenderError("the GPU surface has no supported format".into()))?;
        if let Some(format) = surface
            .get_capabilities(&adapter)
            .formats
            .into_iter()
            .find(wgpu::TextureFormat::is_srgb)
        {
            config.format = format;
        }
        let srgb_target = config.format.is_srgb();
        config.present_mode = PresentMode::Fifo;
        config.alpha_mode = CompositeAlphaMode::Opaque;
        surface.configure(&device, &config);

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

        let mut font_system = FontSystem::new();
        let metrics = CellMetrics::for_scale(window.scale_factor());
        let cell_font_size = fitted_cell_font_size(&mut font_system, metrics);
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
        Ok(Self {
            instance,
            device,
            queue,
            surface,
            config,
            rectangle_pipeline,
            vertices,
            vertex_capacity,
            vertex_count: 0,
            font_system,
            swash_cache,
            viewport,
            atlas,
            text_renderer,
            text: Vec::new(),
            content_key: None,
            clear: color(DEFAULT_BACKGROUND, srgb_target),
            metrics,
            cell_font_size,
            window,
        })
    }

    #[must_use]
    pub fn metrics(&self) -> CellMetrics {
        self.metrics
    }

    #[must_use]
    pub fn size(&self) -> PhysicalSize<u32> {
        PhysicalSize::new(self.config.width, self.config.height)
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>, scale_factor: f64) {
        let size = nonzero(size);
        self.config.width = size.width;
        self.config.height = size.height;
        self.metrics = CellMetrics::for_scale(scale_factor);
        self.cell_font_size = fitted_cell_font_size(&mut self.font_system, self.metrics);
        self.surface.configure(&self.device, &self.config);
        self.content_key = None;
    }

    pub fn render(
        &mut self,
        scene: Option<&Scene>,
        workspace: Option<&WorkspaceScene>,
        workspace_focus: WorkspaceFocus,
        status: &str,
        blink_visible: bool,
        preedit: &str,
    ) -> Result<PresentOutcome, RenderError> {
        self.rebuild_if_needed(
            scene,
            workspace,
            workspace_focus,
            status,
            blink_visible,
            preedit,
        );
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
                &mut self.font_system,
                &mut self.atlas,
                &self.viewport,
                text_areas(&self.text),
                &mut self.swash_cache,
            )
            .is_err()
        {
            self.atlas.trim();
            self.text_renderer
                .prepare(
                    &self.device,
                    &self.queue,
                    &mut self.font_system,
                    &mut self.atlas,
                    &self.viewport,
                    text_areas(&self.text),
                    &mut self.swash_cache,
                )
                .map_err(display_error("the Venus glyph atlas is full"))?;
        }

        let frame = match self.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(frame) => frame,
            CurrentSurfaceTexture::Timeout => {
                self.window.request_redraw();
                return Ok(PresentOutcome::Deferred);
            }
            CurrentSurfaceTexture::Occluded => return Ok(PresentOutcome::Deferred),
            CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                return Ok(PresentOutcome::Recovered);
            }
            CurrentSurfaceTexture::Suboptimal(frame) => {
                drop(frame);
                self.surface.configure(&self.device, &self.config);
                return Ok(PresentOutcome::Recovered);
            }
            CurrentSurfaceTexture::Lost => {
                self.surface = self
                    .instance
                    .create_surface(Arc::clone(&self.window))
                    .map_err(display_error("cannot recover the Venus GPU surface"))?;
                self.surface.configure(&self.device, &self.config);
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
            if self.vertex_count > 0 {
                pass.set_pipeline(&self.rectangle_pipeline);
                pass.set_vertex_buffer(0, self.vertices.slice(..));
                pass.draw(0..self.vertex_count, 0..1);
            }
            self.text_renderer
                .render(&self.atlas, &self.viewport, &mut pass)
                .map_err(display_error("cannot render the Venus glyph atlas"))?;
        }
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(frame);
        self.atlas.trim();
        Ok(PresentOutcome::Presented)
    }

    fn rebuild_if_needed(
        &mut self,
        scene: Option<&Scene>,
        workspace: Option<&WorkspaceScene>,
        workspace_focus: WorkspaceFocus,
        status: &str,
        blink_visible: bool,
        preedit: &str,
    ) {
        let key = ContentKey {
            revision: scene.map(|scene| scene.revision),
            workspace: workspace.cloned(),
            workspace_focus,
            blink_visible,
            preedit: preedit.to_owned(),
            status: status.to_owned(),
        };
        if self.content_key.as_ref() == Some(&key) {
            return;
        }

        self.text.clear();
        let mut rectangles = RectangleBatch::new(self.config.width, self.config.height);
        if let Some(workspace) = workspace {
            self.clear = color(DEFAULT_BACKGROUND, self.config.format.is_srgb());
            self.build_workspace(workspace, workspace_focus, &mut rectangles);
            if let Some(scene) = scene
                && let Some(terminal) = workspace.visible_terminal()
            {
                rectangles.push(
                    terminal.left,
                    terminal.top,
                    terminal.width,
                    terminal.height,
                    scene.background,
                    1.0,
                );
                rectangles.clip = Some(terminal);
                self.build_scene_text(scene, blink_visible, workspace.terminal, terminal);
                build_scene_rectangles(
                    &mut rectangles,
                    scene,
                    blink_visible,
                    self.metrics,
                    workspace.terminal,
                );
                self.build_preedit(
                    scene,
                    preedit,
                    &mut rectangles,
                    workspace.terminal,
                    terminal,
                );
                rectangles.clip = None;
            }
            if workspace_focus == WorkspaceFocus::Terminal
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
                self.build_notice(status, &mut rectangles);
            }
        } else if let Some(scene) = scene {
            self.clear = color(scene.background, self.config.format.is_srgb());
            let viewport = SceneRect {
                left: 0.0,
                top: 0.0,
                width: self.config.width as f32,
                height: self.config.height as f32,
            };
            self.build_scene_text(scene, blink_visible, viewport, viewport);
            build_scene_rectangles(
                &mut rectangles,
                scene,
                blink_visible,
                self.metrics,
                viewport,
            );
            self.build_preedit(scene, preedit, &mut rectangles, viewport, viewport);
            if !status.is_empty() {
                self.build_notice(status, &mut rectangles);
            }
        } else {
            self.clear = color(DEFAULT_BACKGROUND, self.config.format.is_srgb());
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
                DrawStyleKind::Status,
            );
        }
        self.upload_vertices(&rectangles.bytes);
        self.content_key = Some(key);
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
            idle,
            1.0,
        );
        for tab in &workspace.tabs {
            let Some(rect) = tab.rect.intersection(workspace.tab_viewport) else {
                continue;
            };
            rectangles.push(
                rect.left,
                rect.top,
                rect.width,
                rect.height,
                if tab.selected { selected } else { idle },
                1.0,
            );
            if tab.selected {
                rectangles.push(rect.left, rect.bottom() - 3.0, rect.width, 3.0, accent, 1.0);
                if focus == WorkspaceFocus::Tabs {
                    rectangles.push_hollow(
                        rect.left + 2.0,
                        rect.top + 2.0,
                        rect.width - 4.0,
                        rect.height - 4.0,
                        1.0,
                        accent,
                    );
                }
            }
            self.push_text_clipped(
                &tab.id,
                tab.rect.left + self.metrics.padding,
                tab.rect.top + (tab.rect.height - self.metrics.height) / 2.0,
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
                DrawStyleKind::Status,
                workspace.tab_viewport,
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
                if pane.selected { selected } else { idle },
                1.0,
            );
            rectangles.push(
                rect.left,
                rect.bottom() - 1.0,
                rect.width,
                1.0,
                SceneColor {
                    r: 43,
                    g: 53,
                    b: 67,
                },
                1.0,
            );
            if pane.selected && focus == WorkspaceFocus::Panes {
                rectangles.push_hollow(
                    rect.left + 2.0,
                    rect.top + 2.0,
                    rect.width - 4.0,
                    rect.height - 4.0,
                    1.0,
                    accent,
                );
            }
            let label = if pane.live {
                format!("{}  {}", pane.id, pane.session)
            } else {
                format!("{}  {}  offline", pane.id, pane.session)
            };
            self.push_text_clipped(
                &label,
                pane.rect.left + self.metrics.padding,
                pane.rect.top + (pane.rect.height - self.metrics.height) / 2.0,
                pane.rect.width - self.metrics.padding * 2.0,
                self.metrics.height,
                if pane.live {
                    SceneColor {
                        r: 214,
                        g: 222,
                        b: 232,
                    }
                } else {
                    SceneColor {
                        r: 221,
                        g: 126,
                        b: 126,
                    }
                },
                DrawStyleKind::Status,
                workspace.pane_viewport,
            );
        }
    }

    fn build_notice(&mut self, status: &str, rectangles: &mut RectangleBatch) {
        build_notice_rectangles(rectangles, self.metrics);
        self.push_text(
            status,
            self.metrics.padding * 2.0,
            self.config.height as f32 - self.metrics.height * 1.7,
            self.config.width as f32 - self.metrics.padding * 4.0,
            self.metrics.height,
            SceneColor {
                r: 245,
                g: 192,
                b: 94,
            },
            DrawStyleKind::Status,
        );
    }

    fn build_scene_text(
        &mut self,
        scene: &Scene,
        blink_visible: bool,
        origin: SceneRect,
        clip: SceneRect,
    ) {
        for run in scene
            .glyph_runs()
            .into_iter()
            .filter(|run| run.style.foreground_visible(blink_visible))
        {
            let left =
                origin.left + self.metrics.padding + f32::from(run.column) * self.metrics.width;
            let top = origin.top + self.metrics.padding + f32::from(run.row) * self.metrics.height;
            let width = f32::from(run.columns) * self.metrics.width;
            self.push_text_clipped(
                &run.text,
                left,
                top,
                width,
                self.metrics.height,
                run.style.foreground,
                DrawStyleKind::Cell(run.style),
                clip,
            );
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
        layout_height: f32,
        foreground: SceneColor,
        kind: DrawStyleKind,
        clip: SceneRect,
    ) -> f32 {
        let Some(bounds) = (SceneRect {
            left,
            top,
            width: layout_width.max(1.0),
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
        if self.text.len() > previous_len
            && let Some(text) = self.text.last_mut()
        {
            text.bound_left = bounds.left.floor() as i32;
            text.bound_top = bounds.top.floor() as i32;
            text.right = bounds.right().ceil() as i32;
            text.bottom = bounds.bottom().ceil() as i32;
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
                let mut attrs = Attrs::new().family(Family::Monospace);
                if style.bold {
                    attrs = attrs.weight(Weight::BOLD);
                }
                if style.italic {
                    attrs = attrs.style(Style::Italic);
                }
                (
                    self.cell_font_size,
                    self.metrics.height,
                    attrs,
                    Some(self.metrics.width),
                    Wrap::None,
                    if style.faint { 150 } else { 255 },
                )
            }
            DrawStyleKind::Heading => (
                self.metrics.font_size * 1.15,
                self.metrics.height * 1.4,
                Attrs::new().family(Family::SansSerif).weight(Weight::BOLD),
                None,
                Wrap::None,
                255,
            ),
            DrawStyleKind::Preedit => (
                self.cell_font_size,
                self.metrics.height,
                Attrs::new().family(Family::Monospace),
                Some(self.metrics.width),
                Wrap::None,
                255,
            ),
            DrawStyleKind::Status => (
                self.metrics.font_size,
                self.metrics.height * 1.25,
                Attrs::new().family(Family::SansSerif),
                None,
                Wrap::WordOrGlyph,
                255,
            ),
        };
        let layout_width = layout_width.max(1.0);
        let layout_height = layout_height.max(1.0);
        let mut buffer = Buffer::new(&mut self.font_system, Metrics::new(font_size, line_height));
        buffer.set_size(Some(layout_width), Some(layout_height));
        buffer.set_wrap(wrap);
        buffer.set_monospace_width(monospace_width);
        buffer.set_text(text, &attrs, shaping(text), None);
        buffer.shape_until_scroll(&mut self.font_system, false);
        let (left_offset, measured_width) = if matches!(kind, DrawStyleKind::Preedit) {
            shaped_preedit_placement(&buffer)
        } else {
            (0.0, 0.0)
        };
        self.text.push(PlacedText {
            buffer,
            left: left + left_offset,
            top,
            right: (left + layout_width).ceil() as i32,
            bottom: (top + layout_height).ceil() as i32,
            bound_left: left.floor() as i32,
            bound_top: top.floor() as i32,
            color: Color::rgba(foreground.r, foreground.g, foreground.b, alpha),
        });
        measured_width
    }

    fn upload_vertices(&mut self, bytes: &[u8]) {
        self.vertex_count = u32::try_from(bytes.len() / VERTEX_SIZE as usize)
            .expect("bounded Orbit frames fit the vertex count");
        if bytes.is_empty() {
            return;
        }
        let needed = bytes.len() as u64;
        if needed > self.vertex_capacity {
            self.vertex_capacity = needed.next_power_of_two();
            self.vertices = self.device.create_buffer(&BufferDescriptor {
                label: Some("venus rectangle vertices"),
                size: self.vertex_capacity,
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        self.queue.write_buffer(&self.vertices, 0, bytes);
    }
}

#[derive(Clone, Copy)]
enum DrawStyleKind {
    Cell(DrawStyle),
    Heading,
    Preedit,
    Status,
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

fn fitted_cell_font_size(font_system: &mut FontSystem, metrics: CellMetrics) -> f32 {
    let mut buffer = Buffer::new(font_system, Metrics::new(metrics.font_size, metrics.height));
    buffer.set_wrap(Wrap::None);
    buffer.set_text(
        " ",
        &Attrs::new().family(Family::Monospace),
        Shaping::Advanced,
        None,
    );
    let advance = buffer
        .line_layout(font_system, 0)
        .and_then(|lines| lines.first())
        .map(|line| line.w)
        .filter(|width| width.is_finite() && *width > 0.0);
    advance.map_or(metrics.font_size, |advance| {
        (metrics.font_size * metrics.width / advance)
            .round()
            .max(1.0)
    })
}

fn shaping(text: &str) -> Shaping {
    if text.is_ascii() {
        Shaping::Basic
    } else {
        Shaping::Advanced
    }
}

fn text_areas(text: &[PlacedText]) -> impl Iterator<Item = TextArea<'_>> {
    text.iter().map(|text| TextArea {
        buffer: &text.buffer,
        left: text.left,
        top: text.top,
        scale: 1.0,
        bounds: TextBounds {
            left: text.bound_left,
            top: text.bound_top,
            right: text.right,
            bottom: text.bottom,
        },
        default_color: text.color,
        custom_glyphs: &[],
    })
}

struct RectangleBatch {
    bytes: Vec<u8>,
    width: u32,
    height: u32,
    clip: Option<SceneRect>,
}

impl RectangleBatch {
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
        let to_x = |value: f32| value / self.width as f32 * 2.0 - 1.0;
        let to_y = |value: f32| 1.0 - value / self.height as f32 * 2.0;
        let left = to_x(rect.left);
        let right = to_x(rect.right());
        let top = to_y(rect.top);
        let bottom = to_y(rect.bottom());
        let rgba = [
            f32::from(color.r) / 255.0,
            f32::from(color.g) / 255.0,
            f32::from(color.b) / 255.0,
            alpha,
        ];
        for [x, y] in [
            [left, top],
            [left, bottom],
            [right, bottom],
            [left, top],
            [right, bottom],
            [right, top],
        ] {
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
) {
    for (row_index, row) in scene.content.iter().enumerate() {
        let mut start = 0_usize;
        while start < row.cells.len() {
            let background = row.cells[start].style.background;
            let mut end = start + 1;
            while end < row.cells.len() && row.cells[end].style.background == background {
                end += 1;
            }
            if background != scene.background {
                rectangles.push(
                    viewport.left + metrics.padding + start as f32 * metrics.width,
                    viewport.top + metrics.padding + row_index as f32 * metrics.height,
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
            let left = viewport.left + metrics.padding + column as f32 * metrics.width;
            let top = viewport.top + metrics.padding + row_index as f32 * metrics.height;
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
                            top + metrics.height
                                - thickness * if part % 2 == 0 { 3.0 } else { 1.5 },
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
    if let Some(cursor) = scene
        .cursor
        .filter(|cursor| cursor.visible && (blink_visible || !cursor.blinking))
    {
        let column = cursor.leading_column();
        let wide = cursor.at_wide_tail
            || scene
                .content
                .get(usize::from(cursor.row))
                .and_then(|row| row.cells.get(usize::from(column)))
                .is_some_and(|cell| cell.width == CellWidth::Wide);
        let cursor_width = metrics.width * if wide { 2.0 } else { 1.0 };
        let left = viewport.left + metrics.padding + f32::from(column) * metrics.width;
        let top = viewport.top + metrics.padding + f32::from(cursor.row) * metrics.height;
        let thickness = (metrics.width / 7.0).max(1.0);
        let (x, y, w, h, alpha) = match cursor.shape {
            CursorShape::Bar => (left, top, thickness, metrics.height, 1.0),
            CursorShape::Underline => (
                left,
                top + metrics.height - thickness,
                cursor_width,
                thickness,
                1.0,
            ),
            CursorShape::Block => (left, top, cursor_width, metrics.height, 0.55),
            CursorShape::BlockHollow => {
                rectangles.push_hollow(
                    left,
                    top,
                    cursor_width,
                    metrics.height,
                    thickness,
                    cursor.color,
                );
                return;
            }
        };
        rectangles.push(x, y, w, h, cursor.color, alpha);
    }
}

fn build_notice_rectangles(rectangles: &mut RectangleBatch, metrics: CellMetrics) {
    rectangles.push(
        metrics.padding,
        rectangles.height as f32 - metrics.height * 2.0,
        rectangles.width as f32 - metrics.padding * 2.0,
        metrics.height * 1.5,
        SceneColor {
            r: 35,
            g: 29,
            b: 18,
        },
        0.96,
    );
}

fn nonzero(size: PhysicalSize<u32>) -> PhysicalSize<u32> {
    PhysicalSize::new(size.width.max(1), size.height.max(1))
}

fn color(color: SceneColor, srgb_target: bool) -> wgpu::Color {
    let channel = |value| {
        let value = f64::from(value) / 255.0;
        if !srgb_target {
            return value;
        }
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    wgpu::Color {
        r: channel(color.r),
        g: channel(color.g),
        b: channel(color.b),
        a: 1.0,
    }
}

fn display_error<E>(context: &'static str) -> impl FnOnce(E) -> RenderError
where
    E: fmt::Display,
{
    move |error| RenderError(format!("{context}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DrawCell, DrawCursor, DrawRow};
    use orbit_protocol::{CellWidth, Screen};

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
            protected: false,
            underline: Underline::None,
        }
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
        );

        assert!(rectangles.bytes.is_empty());
        assert!(scene.glyph_runs().is_empty());
        assert!(!scene.has_blinking_content());
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
    fn scale_changes_all_cell_metrics_together() {
        assert_eq!(
            CellMetrics::for_scale(2.0),
            CellMetrics {
                width: 18.0,
                height: 36.0,
                font_size: 28.0,
                padding: 24.0,
            }
        );
    }

    #[test]
    fn clear_colors_match_the_surface_color_space() {
        let source = SceneColor {
            r: 128,
            g: 0,
            b: 255,
        };
        let linear = color(source, true);
        let encoded = color(source, false);

        assert!((linear.r - 0.215_860_5).abs() < 0.000_001);
        assert_eq!(linear.g, 0.0);
        assert_eq!(linear.b, 1.0);
        assert_eq!(encoded.r, 128.0 / 255.0);
    }

    #[test]
    fn shaped_preedit_follows_emitted_glyphs_without_charging_combining_marks() {
        fn text_width(text: &str) -> (f32, f32, f32) {
            let mut font_system = FontSystem::new();
            let mut metrics = CellMetrics::for_scale(1.25);
            metrics.font_size = fitted_cell_font_size(&mut font_system, metrics);
            let mut buffer = Buffer::new(
                &mut font_system,
                Metrics::new(metrics.font_size, metrics.height),
            );
            buffer.set_size(Some(100.0), Some(metrics.height));
            buffer.set_wrap(Wrap::None);
            buffer.set_monospace_width(Some(metrics.width));
            buffer.set_text(
                text,
                &Attrs::new().family(Family::Monospace),
                shaping(text),
                None,
            );
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
        }

        for text in ["e", "eee", "e\u{301}", "אבג"] {
            let (width, emitted_width, left) = text_width(text);
            assert!(
                (width - emitted_width).abs() < 0.01,
                "{text:?} measured {width} but emitted {emitted_width}"
            );
            assert!(left.abs() < 0.01, "{text:?} starts at {left}");
        }
        assert!((text_width("e").0 - text_width("e\u{301}").0).abs() < 0.01);
    }

    #[test]
    fn text_runs_follow_the_cell_grid_and_box_borders_connect() {
        fn grid_buffer(
            font_system: &mut FontSystem,
            metrics: CellMetrics,
            text: &str,
            attrs: Attrs<'_>,
        ) -> Buffer {
            let mut buffer =
                Buffer::new(font_system, Metrics::new(metrics.font_size, metrics.height));
            buffer.set_size(
                Some(metrics.width * text.chars().count() as f32),
                Some(metrics.height),
            );
            buffer.set_wrap(Wrap::None);
            buffer.set_monospace_width(Some(metrics.width));
            buffer.set_text(text, &attrs, shaping(text), None);
            buffer.shape_until_scroll(font_system, false);
            buffer
        }

        let mut font_system = FontSystem::new();
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let mut metrics = CellMetrics::for_scale(scale);
            metrics.font_size = fitted_cell_font_size(&mut font_system, metrics);
            for (text, attrs) in [
                ("narrow text", Attrs::new().family(Family::Monospace)),
                (
                    "┌──┬──┐",
                    Attrs::new()
                        .family(Family::Monospace)
                        .weight(Weight::BOLD)
                        .color(Color::rgb(80, 200, 160)),
                ),
                (
                    "│ab│cd│",
                    Attrs::new().family(Family::Monospace).style(Style::Italic),
                ),
            ] {
                let buffer = grid_buffer(&mut font_system, metrics, text, attrs);
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
        }

        let mut metrics = CellMetrics::for_scale(1.0);
        metrics.font_size = fitted_cell_font_size(&mut font_system, metrics);
        let buffer = grid_buffer(
            &mut font_system,
            metrics,
            "─────",
            Attrs::new().family(Family::Monospace),
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
    fn ascii_uses_basic_shaping_without_weakening_unicode() {
        assert_eq!(shaping("plain ASCII"), Shaping::Basic);
        assert_eq!(shaping("e\u{301}"), Shaping::Advanced);
    }
}
