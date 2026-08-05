use crate::{Color as SceneColor, DrawStyle, Scene};
use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, Style,
    SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport, Weight, Wrap,
};
use orbit_protocol::{CursorShape, Underline};
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
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    return input.color;
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
    Presented(Option<u64>),
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
    color: Color,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ContentKey {
    revision: Option<u64>,
    blink_visible: bool,
    preedit: String,
    status: String,
    width: u32,
    height: u32,
    metric_width: u32,
    metric_height: u32,
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
                entry_point: Some("fragment"),
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

        let font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = Cache::new(&device);
        let viewport = Viewport::new(&device, &cache);
        let mut atlas = TextAtlas::new(&device, &queue, &cache, config.format);
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
            clear: color(SceneColor {
                r: 10,
                g: 13,
                b: 20,
            }),
            metrics: CellMetrics::for_scale(window.scale_factor()),
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
        self.surface.configure(&self.device, &self.config);
        self.content_key = None;
    }

    pub fn render(
        &mut self,
        scene: Option<&Scene>,
        status: &str,
        blink_visible: bool,
        preedit: &str,
    ) -> Result<PresentOutcome, RenderError> {
        self.rebuild_if_needed(scene, status, blink_visible, preedit)?;
        self.viewport.update(
            &self.queue,
            Resolution {
                width: self.config.width,
                height: self.config.height,
            },
        );

        let areas = text_areas(&self.text);
        if self
            .text_renderer
            .prepare(
                &self.device,
                &self.queue,
                &mut self.font_system,
                &mut self.atlas,
                &self.viewport,
                areas,
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
            CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => {
                return Ok(PresentOutcome::Deferred);
            }
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
        Ok(PresentOutcome::Presented(scene.map(|scene| scene.revision)))
    }

    fn rebuild_if_needed(
        &mut self,
        scene: Option<&Scene>,
        status: &str,
        blink_visible: bool,
        preedit: &str,
    ) -> Result<(), RenderError> {
        let key = ContentKey {
            revision: scene.map(|scene| scene.revision),
            blink_visible,
            preedit: preedit.to_owned(),
            status: status.to_owned(),
            width: self.config.width,
            height: self.config.height,
            metric_width: self.metrics.width.to_bits(),
            metric_height: self.metrics.height.to_bits(),
        };
        if self.content_key.as_ref() == Some(&key) {
            return Ok(());
        }

        self.text.clear();
        let mut vertices = Vec::new();
        if let Some(scene) = scene {
            self.clear = color(scene.background);
            self.build_scene_text(scene, blink_visible);
            build_scene_rectangles(
                &mut vertices,
                scene,
                blink_visible,
                self.metrics,
                self.config.width,
                self.config.height,
            );
            self.build_preedit(scene, preedit, &mut vertices);
            if !status.is_empty() {
                build_notice_rectangles(
                    &mut vertices,
                    self.metrics,
                    self.config.width,
                    self.config.height,
                );
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
                    DrawStyleKind::Notice,
                );
            }
        } else {
            self.clear = color(SceneColor {
                r: 10,
                g: 13,
                b: 20,
            });
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
        self.upload_vertices(&vertices);
        self.content_key = Some(key);
        Ok(())
    }

    fn build_scene_text(&mut self, scene: &Scene, blink_visible: bool) {
        for run in scene
            .glyph_runs()
            .into_iter()
            .filter(|run| blink_visible || !run.style.blink)
        {
            let left = self.metrics.padding + f32::from(run.column) * self.metrics.width;
            let top = self.metrics.padding + f32::from(run.row) * self.metrics.height;
            let width = f32::from(run.columns) * self.metrics.width;
            self.push_text(
                &run.text,
                left,
                top,
                width,
                self.metrics.height,
                run.style.foreground,
                DrawStyleKind::Cell(run.style),
            );
        }
    }

    fn build_preedit(&mut self, scene: &Scene, preedit: &str, vertices: &mut Vec<u8>) {
        let Some(cursor) = scene.cursor.filter(|_| !preedit.is_empty()) else {
            return;
        };
        let left = self.metrics.padding + f32::from(cursor.column) * self.metrics.width;
        let top = self.metrics.padding + f32::from(cursor.row) * self.metrics.height;
        let width = (self.config.width as f32 - left - self.metrics.padding).max(1.0);
        self.push_text(
            preedit,
            left,
            top,
            width,
            self.metrics.height,
            scene.foreground,
            DrawStyleKind::Preedit,
        );
        push_rect(
            vertices,
            left,
            top + self.metrics.height - 2.0,
            (preedit.chars().count().max(1) as f32 * self.metrics.width).min(width),
            2.0,
            scene.foreground,
            1.0,
            self.config.width,
            self.config.height,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn push_text(
        &mut self,
        text: &str,
        left: f32,
        top: f32,
        width: f32,
        height: f32,
        foreground: SceneColor,
        kind: DrawStyleKind,
    ) {
        if text.is_empty() {
            return;
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
                    self.metrics.font_size,
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
                self.metrics.font_size,
                self.metrics.height,
                Attrs::new().family(Family::Monospace),
                Some(self.metrics.width),
                Wrap::None,
                255,
            ),
            DrawStyleKind::Status | DrawStyleKind::Notice => (
                self.metrics.font_size,
                self.metrics.height * 1.25,
                Attrs::new().family(Family::SansSerif),
                None,
                Wrap::WordOrGlyph,
                255,
            ),
        };
        let mut buffer = Buffer::new(&mut self.font_system, Metrics::new(font_size, line_height));
        buffer.set_size(Some(width.max(1.0)), Some(height.max(1.0)));
        buffer.set_wrap(wrap);
        buffer.set_monospace_width(monospace_width);
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.font_system, false);
        self.text.push(PlacedText {
            buffer,
            left,
            top,
            right: (left + width).ceil() as i32,
            bottom: (top + height).ceil() as i32,
            color: Color::rgba(foreground.r, foreground.g, foreground.b, alpha),
        });
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
    Notice,
}

fn text_areas(text: &[PlacedText]) -> Vec<TextArea<'_>> {
    text.iter()
        .map(|text| TextArea {
            buffer: &text.buffer,
            left: text.left,
            top: text.top,
            scale: 1.0,
            bounds: TextBounds {
                left: text.left.floor() as i32,
                top: text.top.floor() as i32,
                right: text.right,
                bottom: text.bottom,
            },
            default_color: text.color,
            custom_glyphs: &[],
        })
        .collect()
}

fn build_scene_rectangles(
    vertices: &mut Vec<u8>,
    scene: &Scene,
    blink_visible: bool,
    metrics: CellMetrics,
    width: u32,
    height: u32,
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
                push_rect(
                    vertices,
                    metrics.padding + start as f32 * metrics.width,
                    metrics.padding + row_index as f32 * metrics.height,
                    (end - start) as f32 * metrics.width,
                    metrics.height,
                    background,
                    1.0,
                    width,
                    height,
                );
            }
            start = end;
        }
        for (column, cell) in row.cells.iter().enumerate() {
            if cell.style.blink && !blink_visible {
                continue;
            }
            let left = metrics.padding + column as f32 * metrics.width;
            let top = metrics.padding + row_index as f32 * metrics.height;
            let thickness = (metrics.height / 14.0).max(1.0);
            match cell.style.underline {
                Underline::None => {}
                Underline::Single => push_rect(
                    vertices,
                    left,
                    top + metrics.height - thickness * 2.0,
                    metrics.width,
                    thickness,
                    cell.style.underline_color,
                    1.0,
                    width,
                    height,
                ),
                Underline::Curly => {
                    let segment = (metrics.width / 4.0).max(1.0);
                    for part in 0..4 {
                        push_rect(
                            vertices,
                            left + part as f32 * segment,
                            top + metrics.height
                                - thickness * if part % 2 == 0 { 3.0 } else { 1.5 },
                            segment,
                            thickness,
                            cell.style.underline_color,
                            1.0,
                            width,
                            height,
                        );
                    }
                }
                Underline::Dotted => {
                    let dot = thickness.max(1.0);
                    let mut x = left;
                    while x < left + metrics.width {
                        push_rect(
                            vertices,
                            x,
                            top + metrics.height - thickness * 2.0,
                            dot,
                            thickness,
                            cell.style.underline_color,
                            1.0,
                            width,
                            height,
                        );
                        x += dot * 2.0;
                    }
                }
                Underline::Dashed => {
                    let dash = (metrics.width / 3.0).max(1.0);
                    for part in [0.0, 2.0] {
                        push_rect(
                            vertices,
                            left + part * dash,
                            top + metrics.height - thickness * 2.0,
                            dash,
                            thickness,
                            cell.style.underline_color,
                            1.0,
                            width,
                            height,
                        );
                    }
                }
                Underline::Double => {
                    for offset in [2.0, 4.0] {
                        push_rect(
                            vertices,
                            left,
                            top + metrics.height - thickness * offset,
                            metrics.width,
                            thickness,
                            cell.style.underline_color,
                            1.0,
                            width,
                            height,
                        );
                    }
                }
            }
            if cell.style.strikethrough {
                push_rect(
                    vertices,
                    left,
                    top + metrics.height * 0.52,
                    metrics.width,
                    thickness,
                    cell.style.foreground,
                    1.0,
                    width,
                    height,
                );
            }
            if cell.style.overline {
                push_rect(
                    vertices,
                    left,
                    top + thickness,
                    metrics.width,
                    thickness,
                    cell.style.foreground,
                    1.0,
                    width,
                    height,
                );
            }
        }
    }
    if let Some(cursor) = scene
        .cursor
        .filter(|cursor| cursor.visible && (blink_visible || !cursor.blinking))
    {
        let left = metrics.padding + f32::from(cursor.column) * metrics.width;
        let top = metrics.padding + f32::from(cursor.row) * metrics.height;
        let thickness = (metrics.width / 7.0).max(1.0);
        let (x, y, w, h, alpha) = match cursor.shape {
            CursorShape::Bar => (left, top, thickness, metrics.height, 1.0),
            CursorShape::Underline => (
                left,
                top + metrics.height - thickness,
                metrics.width,
                thickness,
                1.0,
            ),
            CursorShape::Block => (left, top, metrics.width, metrics.height, 0.55),
            CursorShape::BlockHollow => {
                push_hollow_rect(
                    vertices,
                    left,
                    top,
                    metrics.width,
                    metrics.height,
                    thickness,
                    cursor.color,
                    width,
                    height,
                );
                return;
            }
        };
        push_rect(vertices, x, y, w, h, cursor.color, alpha, width, height);
    }
}

fn build_notice_rectangles(vertices: &mut Vec<u8>, metrics: CellMetrics, width: u32, height: u32) {
    push_rect(
        vertices,
        metrics.padding,
        height as f32 - metrics.height * 2.0,
        width as f32 - metrics.padding * 2.0,
        metrics.height * 1.5,
        SceneColor {
            r: 35,
            g: 29,
            b: 18,
        },
        0.96,
        width,
        height,
    );
}

#[allow(clippy::too_many_arguments)]
fn push_hollow_rect(
    vertices: &mut Vec<u8>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    thickness: f32,
    color: SceneColor,
    screen_width: u32,
    screen_height: u32,
) {
    for (x, y, width, height) in [
        (x, y, width, thickness),
        (x, y + height - thickness, width, thickness),
        (x, y, thickness, height),
        (x + width - thickness, y, thickness, height),
    ] {
        push_rect(
            vertices,
            x,
            y,
            width,
            height,
            color,
            1.0,
            screen_width,
            screen_height,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn push_rect(
    vertices: &mut Vec<u8>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: SceneColor,
    alpha: f32,
    screen_width: u32,
    screen_height: u32,
) {
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let to_x = |value: f32| value / screen_width as f32 * 2.0 - 1.0;
    let to_y = |value: f32| 1.0 - value / screen_height as f32 * 2.0;
    let left = to_x(x);
    let right = to_x(x + width);
    let top = to_y(y);
    let bottom = to_y(y + height);
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
            vertices.extend_from_slice(&value.to_ne_bytes());
        }
    }
}

fn nonzero(size: PhysicalSize<u32>) -> PhysicalSize<u32> {
    PhysicalSize::new(size.width.max(1), size.height.max(1))
}

fn color(color: SceneColor) -> wgpu::Color {
    wgpu::Color {
        r: f64::from(color.r) / 255.0,
        g: f64::from(color.g) / 255.0,
        b: f64::from(color.b) / 255.0,
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

    #[test]
    fn rectangle_encoding_is_row_major_and_bounded() {
        let mut bytes = Vec::new();
        push_rect(
            &mut bytes,
            0.0,
            0.0,
            10.0,
            20.0,
            SceneColor { r: 1, g: 2, b: 3 },
            1.0,
            100,
            100,
        );
        assert_eq!(
            bytes.len(),
            VERTEX_SIZE as usize * VERTICES_PER_QUAD as usize
        );
        assert_eq!(&bytes[..4], &(-1.0_f32).to_ne_bytes());
        assert_eq!(&bytes[4..8], &1.0_f32.to_ne_bytes());
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
}
