use crate::{Result, launch::LaunchArguments};
use accesskit::Action as AccessibilityAction;
use accesskit_winit::{Event as AccessKitEvent, WindowEvent as AccessKitWindowEvent};
use eon_workspace_protocol::v4::{Action as WorkspaceAction, Direction as WorkspaceDirection};
use orbit_protocol::{
    MAX_CELLS,
    session::{
        self, ClientMessage, ClipboardLocation, FailureCode, MAX_SCROLL_ROWS, ScrollOutcome,
        SelectionAction, ServerMessage, SurfaceSize, VerticalDirection, WheelOutcome,
    },
};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    ffi::OsString,
    io::{self, Read, Write},
    os::unix::ffi::OsStringExt,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
#[cfg(target_os = "linux")]
use winit::platform::wayland::WindowAttributesExtWayland;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalPosition, PhysicalSize},
    event::{ElementState, Ime, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorIcon, UserAttentionType, Window, WindowAttributes, WindowId},
};
use yazelix_venus::{
    Accessibility, AccessibilityTarget, CellMetrics, ClipboardEffect, Color, ConnectionState,
    FontSettings, FontSetup, Hyperlink, InputState, LocalNoticeSource, MetadataEvent,
    MetadataTransport, ModelError, PaneMetadata, PresentOutcome, Renderer, Scene, ScenePreview,
    SceneRect, SessionModel, Transport, TransportEvent, WorkspaceEvent, WorkspaceFocus,
    WorkspaceHit, WorkspaceModel, WorkspaceScene, WorkspaceTransport, directory_picker_visible,
};

const BLINK_INTERVAL: Duration = Duration::from_millis(500);
const ANIMATION_FRAME_INTERVAL: Duration = Duration::from_millis(16);
const INITIAL_RETRY_DELAY: Duration = Duration::from_millis(250);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(5);
const SCROLL_SAMPLE_WINDOW: Duration = Duration::from_millis(150);
const MAX_SCROLL_SAMPLES: usize = 256;
const MAX_DEFERRED_SELECTION_MESSAGES: usize = 256;
const PRECISION_SCROLL_GAIN: f64 = 2.0;
const SCROLL_DECAY: f64 = 4.0;
const MIN_FLING_VELOCITY: f64 = 40.0;
const MAX_FLING_VELOCITY: f64 = 8_000.0;

#[derive(Debug, Default)]
struct TerminalScroll {
    pixels: f64,
    line_fraction: f64,
    terminal_lines: f64,
    samples: VecDeque<(Instant, f64)>,
    phase_active: bool,
    velocity: f64,
    last_advance: Option<Instant>,
    preview_pending: Option<(u64, VerticalDirection)>,
    in_flight: Option<i16>,
    batch_cancelled: bool,
}

impl TerminalScroll {
    fn push_lines(&mut self, lines: f64, cell_height: f64) {
        if !lines.is_finite() || !cell_height.is_finite() || cell_height <= 0.0 {
            return;
        }
        self.stop_gesture();
        self.terminal_lines += lines;
        self.line_fraction += lines;
        let whole = self.line_fraction.trunc();
        self.line_fraction -= whole;
        self.pixels += whole * 3.0 * cell_height;
    }

    fn push_pixels(&mut self, pixels: f64, phase: TouchPhase, now: Instant, cell_height: f64) {
        if !pixels.is_finite() || !cell_height.is_finite() || cell_height <= 0.0 {
            return;
        }
        let native_pixels = pixels;
        let pixels = native_pixels * PRECISION_SCROLL_GAIN;
        match phase {
            TouchPhase::Started => {
                self.stop_gesture();
                self.phase_active = true;
                self.record_sample(now, pixels);
            }
            TouchPhase::Moved => {
                if !self.phase_active {
                    // A discrete Wayland axis may leave winit's shared phase at Moved.
                    self.stop_gesture();
                    self.phase_active = true;
                }
                self.record_sample(now, pixels);
            }
            TouchPhase::Ended if self.phase_active => {
                self.record_sample(now, pixels);
                self.phase_active = false;
                self.velocity = self.release_velocity();
                self.last_advance = (self.velocity != 0.0).then_some(now);
            }
            TouchPhase::Ended | TouchPhase::Cancelled => self.stop_gesture(),
        }
        if phase != TouchPhase::Cancelled {
            self.terminal_lines += native_pixels / cell_height;
            self.pixels += pixels;
        }
    }

    fn record_sample(&mut self, now: Instant, pixels: f64) {
        while self.samples.len() >= MAX_SCROLL_SAMPLES
            || self.samples.front().is_some_and(|(time, _)| {
                now.saturating_duration_since(*time) > SCROLL_SAMPLE_WINDOW
            })
        {
            self.samples.pop_front();
        }
        self.samples.push_back((now, pixels));
    }

    fn release_velocity(&mut self) -> f64 {
        let velocity = self
            .samples
            .front()
            .zip(self.samples.back())
            .and_then(|((first, _), (last, _))| {
                let seconds = last.saturating_duration_since(*first).as_secs_f64();
                (seconds > 0.0)
                    .then(|| self.samples.iter().map(|(_, pixels)| pixels).sum::<f64>() / seconds)
            })
            .unwrap_or(0.0)
            .clamp(-MAX_FLING_VELOCITY, MAX_FLING_VELOCITY);
        self.samples.clear();
        if velocity.abs() < MIN_FLING_VELOCITY {
            0.0
        } else {
            velocity
        }
    }

    fn advance(&mut self, now: Instant) {
        let Some(last) = self.last_advance else {
            return;
        };
        self.last_advance = Some(now);
        let elapsed = now.saturating_duration_since(last).as_secs_f64();
        let next_velocity = self.velocity * (-SCROLL_DECAY * elapsed).exp();
        self.pixels += (self.velocity - next_velocity) / SCROLL_DECAY;
        self.velocity = next_velocity;
        if self.velocity.abs() / SCROLL_DECAY < 1.0 {
            self.stop_fling();
        }
    }

    fn stop_fling(&mut self) {
        self.velocity = 0.0;
        self.last_advance = None;
    }

    fn stop_gesture(&mut self) {
        self.samples.clear();
        self.phase_active = false;
        self.stop_fling();
    }

    fn cancel(&mut self) {
        // A written batch still needs its ordered response before another request.
        let in_flight = self.in_flight;
        *self = Self::default();
        self.in_flight = in_flight;
        self.batch_cancelled = in_flight.is_some();
    }

    fn reset(&mut self) {
        *self = Self::default();
    }

    fn direction(&self) -> Option<VerticalDirection> {
        if self.pixels > 0.0 {
            Some(VerticalDirection::Up)
        } else if self.pixels < 0.0 {
            Some(VerticalDirection::Down)
        } else {
            None
        }
    }

    fn preview_arrived(&mut self, revision: u64, direction: VerticalDirection) {
        if self.preview_pending == Some((revision, direction)) {
            self.preview_pending = None;
        }
    }

    fn resolve_preview(&mut self, preview: Option<&ScenePreview>) -> Option<f64> {
        match preview {
            Some(ScenePreview::TerminalOwned { direction, .. })
                if Some(*direction) == self.direction() =>
            {
                self.pixels = 0.0;
                self.line_fraction = 0.0;
                self.stop_gesture();
                Some(std::mem::take(&mut self.terminal_lines))
            }
            Some(ScenePreview::Viewport {
                direction,
                edge_reached,
                rows,
                ..
            }) if Some(*direction) == self.direction() => {
                self.terminal_lines = 0.0;
                if *edge_reached && rows.is_empty() {
                    self.pixels = 0.0;
                    self.stop_gesture();
                }
                None
            }
            _ => None,
        }
    }

    fn next_request(
        &mut self,
        frame_revision: u64,
        preview: Option<&ScenePreview>,
        cell_height: f64,
    ) -> Option<ClientMessage> {
        if self.in_flight.is_some() || !cell_height.is_finite() || cell_height <= 0.0 {
            return None;
        }
        let direction = self.direction()?;
        let (preview_revision, rows) = match preview {
            Some(ScenePreview::Viewport {
                frame_revision: revision,
                direction: candidate,
                rows,
                ..
            }) if *candidate == direction => (*revision, rows),
            _ => {
                if self.preview_pending != Some((frame_revision, direction)) {
                    self.preview_pending = Some((frame_revision, direction));
                    return Some(ClientMessage::PreviewVertical {
                        frame_revision,
                        direction,
                    });
                }
                return None;
            }
        };
        let available = i64::try_from(rows.len())
            .unwrap_or(i64::MAX)
            .min(i64::from(MAX_SCROLL_ROWS));
        if available == 0 {
            return None;
        }
        let crossed = (self.pixels / cell_height).trunc();
        if crossed == 0.0 {
            return None;
        }
        let requested = (-(crossed as i64)).clamp(-available, available) as i16;
        self.in_flight = Some(requested);
        Some(ClientMessage::ScrollVertical {
            frame_revision: preview_revision,
            rows: requested,
        })
    }

    fn accept_batch(&mut self, requested_rows: i16, applied_rows: i16, cell_height: f64) -> bool {
        if self.in_flight != Some(requested_rows) {
            return false;
        }
        self.in_flight = None;
        self.preview_pending = None;
        if !std::mem::take(&mut self.batch_cancelled) {
            self.pixels += f64::from(applied_rows) * cell_height;
        }
        true
    }

    fn rebase(&mut self) {
        self.preview_pending = None;
    }

    fn offset(&self, preview: Option<&ScenePreview>, cell_height: f64) -> f32 {
        let Some(direction) = self.direction() else {
            return 0.0;
        };
        let Some(ScenePreview::Viewport {
            direction: candidate,
            rows,
            ..
        }) = preview
        else {
            return 0.0;
        };
        if *candidate != direction || rows.is_empty() {
            return 0.0;
        }
        let limit = rows.len() as f64 * cell_height;
        self.pixels.clamp(-limit, limit) as f32
    }

    fn active(&self) -> bool {
        self.velocity != 0.0
            || self.pixels != 0.0
            || self.preview_pending.is_some()
            || self.in_flight.is_some() && !self.batch_cancelled
    }
}

struct OrbitRetry {
    deadline: Option<Instant>,
    next_delay: Duration,
}

impl Default for OrbitRetry {
    fn default() -> Self {
        Self {
            deadline: None,
            next_delay: INITIAL_RETRY_DELAY,
        }
    }
}

impl OrbitRetry {
    fn schedule(&mut self, now: Instant) {
        if self.deadline.is_none() {
            self.deadline = Some(now + self.next_delay);
            self.next_delay = self.next_delay.saturating_mul(2).min(MAX_RETRY_DELAY);
        }
    }

    fn take_due(&mut self, now: Instant) -> bool {
        if self.deadline.is_some_and(|deadline| now >= deadline) {
            self.deadline = None;
            true
        } else {
            false
        }
    }

    fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PresentationIdentity {
    generation: u64,
    revision: Option<u64>,
}

#[derive(Debug, Default)]
struct PresentationState {
    generation: u64,
    presented: Option<PresentationIdentity>,
}

impl PresentationState {
    fn candidate(&self, revision: Option<u64>) -> PresentationIdentity {
        PresentationIdentity {
            generation: self.generation,
            revision,
        }
    }

    fn content_changed(&mut self) {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("presentation generation exhausted");
    }

    fn invalidate(&mut self) {
        self.content_changed();
        self.unpublish();
    }

    fn publish(&mut self, candidate: PresentationIdentity) {
        if candidate.generation == self.generation {
            self.presented = Some(candidate);
        }
    }

    fn unpublish(&mut self) {
        self.presented = None;
    }

    fn has_presented_geometry(&self) -> bool {
        self.presented.is_some()
    }

    fn presented_revision(&self) -> Option<u64> {
        self.presented.and_then(|identity| identity.revision)
    }

    fn is_current(&self, identity: PresentationIdentity) -> bool {
        identity.revision.is_some()
            && identity.generation == self.generation
            && self.presented == Some(identity)
    }
}

const MAX_LINK_BYTES: usize = 4096;

fn validate_copy_uri(uri: &str) -> std::result::Result<(), &'static str> {
    if uri.is_empty() || uri.len() > MAX_LINK_BYTES {
        Err("Link target must contain 1–4096 bytes.")
    } else if uri.chars().any(char::is_control) {
        Err("Link target contains control characters.")
    } else {
        Ok(())
    }
}

fn validate_open_uri(uri: &str) -> std::result::Result<(), &'static str> {
    validate_copy_uri(uri)?;
    let malformed =
        "Cannot open this link: expected an ASCII HTTP/HTTPS URI with a host and no credentials.";
    let (scheme, rest) = uri.split_once(':').ok_or(malformed)?;
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return Err("Only HTTP and HTTPS links can be opened. Copy other targets explicitly.");
    }
    let rest = rest.strip_prefix("//").ok_or(malformed)?;
    // A deliberately narrow serialized-URI policy, not URL discovery or normalization.
    let bytes = uri.as_bytes();
    for (i, &byte) in bytes.iter().enumerate() {
        if !byte.is_ascii_alphanumeric() && !b"-._~:/?#[]@!$&'()*+,;=%".contains(&byte) {
            return Err(malformed);
        }
        if byte == b'%'
            && !bytes
                .get(i + 1..i + 3)
                .is_some_and(|hex| hex.iter().all(u8::is_ascii_hexdigit))
        {
            return Err("Link target contains an invalid percent escape.");
        }
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let (host, port) = if let Some(ipv6) = authority.strip_prefix('[') {
        let (host, suffix) = ipv6.split_once(']').ok_or(malformed)?;
        host.parse::<std::net::Ipv6Addr>().map_err(|_| malformed)?;
        (host, suffix)
    } else {
        let (host, port) = authority.split_at(authority.find(':').unwrap_or(authority.len()));
        if !host
            .strip_suffix('.')
            .unwrap_or(host)
            .split('.')
            .all(|label| {
                !label.is_empty()
                    && label.len() <= 63
                    && !label.starts_with('-')
                    && !label.ends_with('-')
                    && label
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            })
            || host.len() > 253
        {
            return Err(malformed);
        }
        (host, port)
    };
    if host.is_empty()
        || (!port.is_empty()
            && !port.strip_prefix(':').is_some_and(|port| {
                !port.is_empty()
                    && port.bytes().all(|c| c.is_ascii_digit())
                    && port.parse::<u16>().is_ok()
            }))
    {
        return Err(malformed);
    }
    Ok(())
}

fn escaped_link(uri: &str) -> String {
    uri.chars()
        .take(MAX_LINK_BYTES)
        .flat_map(char::escape_default)
        .collect()
}

fn link_page(uri: &str, page: usize, width: usize) -> &str {
    let start = page.saturating_mul(width).min(uri.len());
    &uri[start..start.saturating_add(width).min(uri.len())]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LinkFocus {
    identity: PresentationIdentity,
    row: u16,
    column: u16,
}

#[derive(Default)]
struct LinkInteraction {
    focus: Option<LinkFocus>,
    presented: Option<LinkFocus>,
    unshifted: bool,
    pointer_inside: bool,
    keyboard: bool,
    page: usize,
    pressed: Option<LinkFocus>,
    notice: Option<String>,
}

struct LinkOpener {
    child: Child,
    deadline: Instant,
}

impl LinkOpener {
    fn start(uri: &str) -> std::result::Result<Self, &'static str> {
        validate_open_uri(uri)?;
        let child = Command::new("gio")
            .args(["open", "--", uri])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "Cannot open link: install GIO (gio) on the desktop host PATH.")?;
        Ok(Self {
            child,
            deadline: Instant::now() + Duration::from_secs(10),
        })
    }

    fn poll(&mut self, now: Instant) -> Option<&'static str> {
        match self.child.try_wait() {
            Ok(Some(status)) if status.success() => Some("Link handed to the desktop handler."),
            Ok(Some(_)) => Some("Could not open link. Check the desktop's HTTP/HTTPS handler."),
            Err(_) => Some("Could not observe the native link dispatcher."),
            Ok(None) if now >= self.deadline => {
                Some("Link dispatcher timed out. The handler may already have opened.")
            }
            Ok(None) => None,
        }
    }
}

impl Drop for LinkOpener {
    fn drop(&mut self) {
        // GIO dispatches a separate handler; retire only our dispatcher, never the browser.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
enum SelectionGate {
    #[default]
    Ready,
    AwaitingFinish,
    AwaitingPresentation(u64),
}

impl SelectionGate {
    fn finish_sent(&mut self) {
        *self = Self::AwaitingFinish;
    }

    fn finish_received(&mut self, frame_revision: u64) {
        if matches!(self, Self::AwaitingFinish) {
            *self = Self::AwaitingPresentation(frame_revision);
        }
    }

    fn admits(&mut self, presented_revision: u64) -> bool {
        if matches!(self, Self::AwaitingPresentation(required) if presented_revision >= *required) {
            *self = Self::Ready;
        }
        matches!(self, Self::Ready)
    }
}

#[derive(Debug)]
enum UserEvent {
    AccessKit(AccessKitEvent),
    Exit,
    Present,
    Metadata,
    Transport,
    Workspace,
}

impl From<AccessKitEvent> for UserEvent {
    fn from(value: AccessKitEvent) -> Self {
        Self::AccessKit(value)
    }
}

struct WindowState {
    renderer: Renderer,
    scale_factor: f64,
    adapter: accesskit_winit::Adapter,
    accessibility: Accessibility,
    window: Arc<Window>,
}

struct MetadataObserver {
    transport: MetadataTransport,
    metadata: PaneMetadata,
}

struct Application {
    application_id: String,
    orbit_socket: Option<PathBuf>,
    workspace_socket: Option<PathBuf>,
    supervised: bool,
    startup_admission: bool,
    decorations: bool,
    pane_frames: bool,
    background_opacity: f32,
    background_blur: bool,
    cursor_tail: Option<(Color, f32)>,
    fonts: FontSettings,
    columns: Option<u16>,
    rows: Option<u16>,
    fatal_error: Option<String>,
    initial_scale_pending: bool,
    proxy: EventLoopProxy<UserEvent>,
    window: Option<WindowState>,
    transport: Option<Transport>,
    workspace_transport: Option<WorkspaceTransport>,
    metadata_observers: HashMap<Vec<u8>, MetadataObserver>,
    model: SessionModel,
    workspace_model: WorkspaceModel,
    input: InputState,
    deferred_selection: VecDeque<ClientMessage>,
    selection_gate: SelectionGate,
    input_epoch: Instant,
    last_resize: Option<SurfaceSize>,
    presentation: PresentationState,
    links: LinkInteraction,
    link_opener: Option<LinkOpener>,
    render_notice: Option<String>,
    blink_visible: bool,
    next_blink: Option<Instant>,
    next_animation: Option<Instant>,
    active_endpoint: Option<Vec<u8>>,
    active_endpoint_live: bool,
    orbit_retry: OrbitRetry,
    retry_suppressed: bool,
    window_focused: bool,
    window_occluded: bool,
    workspace_focus: WorkspaceFocus,
    tab_texts: HashMap<String, (String, f32)>,
    tab_scroll: f32,
    pane_scroll: f32,
    terminal_scroll: TerminalScroll,
    cursor: PhysicalPosition<f64>,
}

impl Application {
    fn new(arguments: LaunchArguments, proxy: EventLoopProxy<UserEvent>) -> Self {
        let LaunchArguments {
            application_id,
            orbit_socket,
            workspace_socket,
            supervised,
            startup_admission,
            decorations,
            pane_frames,
            background_opacity,
            background_blur,
            cursor_tail,
            fonts,
            columns,
            rows,
        } = arguments;
        let initial_scale_pending = startup_admission
            || columns.is_some()
            || rows.is_some()
            || fonts != FontSettings::default();
        Self {
            application_id,
            orbit_socket,
            workspace_socket,
            supervised,
            startup_admission,
            decorations,
            pane_frames,
            background_opacity,
            background_blur,
            cursor_tail,
            fonts,
            columns,
            rows,
            proxy,
            fatal_error: None,
            initial_scale_pending,
            window: None,
            transport: None,
            workspace_transport: None,
            metadata_observers: HashMap::new(),
            model: SessionModel::new(),
            workspace_model: WorkspaceModel::default(),
            input: InputState::default(),
            deferred_selection: VecDeque::new(),
            selection_gate: SelectionGate::default(),
            input_epoch: Instant::now(),
            last_resize: None,
            presentation: PresentationState::default(),
            links: LinkInteraction::default(),
            link_opener: None,
            render_notice: None,
            blink_visible: true,
            next_blink: None,
            next_animation: None,
            active_endpoint: None,
            active_endpoint_live: false,
            orbit_retry: OrbitRetry::default(),
            retry_suppressed: false,
            window_focused: false,
            window_occluded: false,
            workspace_focus: WorkspaceFocus::Terminal,
            tab_texts: HashMap::new(),
            tab_scroll: 0.0,
            pane_scroll: 0.0,
            terminal_scroll: TerminalScroll::default(),
            cursor: PhysicalPosition::new(0.0, 0.0),
        }
    }

    fn create_window(&mut self, event_loop: &ActiveEventLoop) -> Result {
        let fonts = FontSetup::new(&self.fonts)?;
        let initial = initial_window_size(
            self.columns,
            self.rows,
            fonts.metrics(1.0),
            1.0,
            self.workspace_model.snapshot(),
        )?;
        let attributes = window_attributes(
            self.decorations,
            self.background_opacity,
            self.background_blur,
        )
        .with_inner_size(initial.to_logical::<f64>(1.0));
        #[cfg(target_os = "linux")]
        let attributes = attributes.with_name(self.application_id.as_str(), "yazelix-venus");
        let window = Arc::new(event_loop.create_window(attributes)?);
        if self.initial_scale_pending && window.scale_factor() != 1.0 {
            let initial = initial_window_size(
                self.columns,
                self.rows,
                fonts.metrics(window.scale_factor()),
                window.scale_factor(),
                self.workspace_model.snapshot(),
            )?;
            let _ = window.request_inner_size(initial);
        }
        let accessibility = Accessibility::new(window.inner_size());
        let adapter = accesskit_winit::Adapter::with_mixed_handlers(
            event_loop,
            &window,
            accessibility.activation(),
            self.proxy.clone(),
        );
        let renderer = pollster::block_on(Renderer::new(
            Arc::clone(&window),
            event_loop,
            self.background_opacity,
            self.cursor_tail,
            self.pane_frames,
            fonts,
        ))?;
        let scale_factor = window.scale_factor();

        self.window = Some(WindowState {
            renderer,
            scale_factor,
            adapter,
            accessibility,
            window,
        });
        if !self.initial_scale_pending {
            self.start_initial_attachment();
        } else {
            self.reveal_workspace_selection();
        }
        self.refresh_client_view();
        Ok(())
    }

    fn start_initial_attachment(&mut self) {
        if self.workspace_transport.is_none()
            && let Some(socket) = self.workspace_socket.clone()
        {
            self.start_workspace(socket);
        }
        self.reveal_workspace_selection();
        if self.workspace_socket.is_some() {
            if let Some((endpoint, live)) = self.workspace_model.active_attachment() {
                self.set_orbit_attachment(endpoint.to_vec(), live);
            }
        } else {
            self.start_orbit(
                self.orbit_socket
                    .clone()
                    .expect("standalone launch has an Orbit socket"),
            );
        }
    }

    fn start_workspace(&mut self, socket: PathBuf) {
        let proxy = self.proxy.clone();
        self.workspace_transport = Some(WorkspaceTransport::start(socket, move || {
            let _ = proxy.send_event(UserEvent::Workspace);
        }));
    }

    fn start_orbit(&mut self, socket: PathBuf) {
        let proxy = self.proxy.clone();
        self.orbit_socket = Some(socket.clone());
        self.transport = Some(Transport::start(socket, move || {
            let _ = proxy.send_event(UserEvent::Transport);
        }));
    }

    fn reset_cursor_animation(&mut self) {
        self.next_animation = None;
        if let Some(state) = &mut self.window {
            state.renderer.reset_cursor_animation();
        }
    }

    fn cancel_terminal_scroll(&mut self) {
        let redraw = self.terminal_scroll.active();
        self.terminal_scroll.cancel();
        if !redraw {
            return;
        }
        if let Some(state) = &mut self.window {
            state.renderer.reset_cursor_animation();
            state.window.request_redraw();
        }
    }

    fn drive_terminal_scroll(&mut self) {
        let Some(frame_revision) = self.model.scene().map(|scene| scene.revision) else {
            self.terminal_scroll.reset();
            return;
        };
        let Some(metrics) = self.window.as_ref().map(|state| state.renderer.metrics()) else {
            return;
        };
        let (lines, request) = {
            let preview = self.model.scroll_preview();
            (
                self.terminal_scroll.resolve_preview(preview),
                self.terminal_scroll.next_request(
                    frame_revision,
                    preview,
                    f64::from(metrics.height),
                ),
            )
        };
        if let Some(lines) = lines {
            for message in self.input.wheel(
                MouseScrollDelta::LineDelta(0.0, lines as f32),
                metrics.width,
                metrics.height,
            ) {
                if !self.send(message) {
                    break;
                }
            }
        }
        if let Some(message) = request
            && !self.send(message)
        {
            self.terminal_scroll.reset();
        }
        if !self.terminal_scroll.active() {
            self.model.clear_viewport_preview();
        }
    }

    fn workspace_scene(&self) -> Option<WorkspaceScene> {
        let state = self.window.as_ref()?;
        self.workspace_model.snapshot().map(|snapshot| {
            WorkspaceScene::from_snapshot_with_metadata(
                snapshot,
                state.renderer.size(),
                state.renderer.metrics(),
                self.tab_scroll,
                self.pane_scroll,
                |endpoint| {
                    self.metadata_observers
                        .get(endpoint)
                        .map(|observer| &observer.metadata)
                },
                |id, _| self.tab_texts[id].clone(),
            )
        })
    }

    fn terminal_size(&self) -> Option<PhysicalSize<u32>> {
        let state = self.window.as_ref()?;
        Some(terminal_screen(
            self.workspace_scene().as_ref(),
            state.renderer.size(),
        ))
    }

    fn scrollback_label(&self, workspace: Option<&WorkspaceScene>) -> Option<String> {
        let state = self.window.as_ref()?;
        let scene = self.model.scene()?;
        let size = surface_size(
            terminal_screen(workspace, state.renderer.size()),
            state.renderer.metrics(),
        )?;
        if scene.columns != size.cols || scene.rows != size.rows {
            return None;
        }
        self.model.scrollback_label()
    }

    fn presentation_candidate(&self, workspace: Option<&WorkspaceScene>) -> PresentationIdentity {
        let revision = self
            .model
            .scene()
            .filter(|_| self.model.is_attached() && !self.model.awaiting_current_frame())
            .and_then(|scene| {
                let state = self.window.as_ref()?;
                let screen = terminal_screen(workspace, state.renderer.size());
                surface_size(screen, state.renderer.metrics()).map(|_| scene.revision)
            });
        self.presentation.candidate(revision)
    }

    fn reveal_workspace_selection(&mut self) {
        let Some(state) = &mut self.window else {
            return;
        };
        let Some(snapshot) = self.workspace_model.snapshot() else {
            return;
        };
        self.tab_texts.clear();
        let scene = WorkspaceScene::from_snapshot(
            snapshot,
            state.renderer.size(),
            state.renderer.metrics(),
            0.0,
            0.0,
            |id, label| {
                let fitted = state.renderer.fit_tab_text(label);
                self.tab_texts.insert(id.to_owned(), fitted.clone());
                fitted
            },
        );
        self.tab_scroll = scene.active_tab_scroll();
        self.pane_scroll = scene.selected_pane_scroll();
    }

    fn set_workspace_focus(&mut self, focus: WorkspaceFocus) {
        if self.workspace_model.directory_picker_visible() || self.workspace_focus == focus {
            return;
        }
        let was_focused = terminal_focused(self.window_focused, self.workspace_focus);
        self.cancel_terminal_scroll();
        self.workspace_focus = focus;
        let is_focused = terminal_focused(self.window_focused, self.workspace_focus);
        if was_focused != is_focused {
            if !is_focused {
                self.cancel_pointer_sequence();
            }
            let message = self.input.terminal_focus(is_focused);
            self.send(message);
        }
        self.refresh_client_view();
    }

    fn set_orbit_attachment(&mut self, endpoint: Vec<u8>, live: bool) {
        self.terminal_scroll.reset();
        self.deferred_selection.clear();
        self.selection_gate = SelectionGate::Ready;
        if let Some(message) = self.input.retire_orbit_generation() {
            self.send(message);
        }
        self.reset_cursor_animation();
        let same_endpoint = self.active_endpoint.as_ref() == Some(&endpoint);
        self.active_endpoint = Some(endpoint.clone());
        self.active_endpoint_live = live;
        self.transport = None;
        self.orbit_retry.reset();
        self.retry_suppressed = !live;
        if same_endpoint {
            self.model.prepare_reconnect();
        } else {
            self.model = SessionModel::new();
        }
        if !live {
            self.model.mark_lost("The selected Eon pane is offline");
        }
        self.last_resize = None;
        self.presentation.invalidate();
        if live {
            self.start_orbit(PathBuf::from(OsString::from_vec(endpoint)));
        }
    }

    fn retry_allowed(&self) -> bool {
        retry_is_allowed(
            self.workspace_socket.is_some(),
            self.workspace_model.active_attachment(),
            self.active_endpoint.as_deref(),
        )
    }

    fn retry_orbit(&mut self, now: Instant) {
        if !self.orbit_retry.take_due(now) {
            return;
        }
        if self.transport.is_some() || !self.retry_allowed() {
            self.orbit_retry.reset();
            return;
        }
        self.model.prepare_reconnect();
        self.terminal_scroll.reset();
        self.reset_cursor_animation();
        self.retry_suppressed = false;
        self.last_resize = None;
        self.presentation.invalidate();
        self.start_orbit(
            self.orbit_socket
                .clone()
                .expect("Orbit retry has an attachment socket"),
        );
        self.refresh_client_view();
    }

    fn handle_workspace(&mut self, event: WorkspaceEvent) {
        let picker_was_visible = self.workspace_model.directory_picker_visible();
        let received_snapshot = matches!(
            &event,
            WorkspaceEvent::Response(eon_workspace_protocol::v4::Response::Snapshot(_))
        );
        let unavailable = matches!(&event, WorkspaceEvent::Unavailable(_));
        let (view_changed, snapshot_changed) = match event {
            WorkspaceEvent::Response(response) => self.workspace_model.apply(response),
            WorkspaceEvent::Unavailable(detail) => {
                (self.workspace_model.mark_unavailable(detail), false)
            }
        };
        if self.window.is_none() || self.initial_scale_pending {
            if snapshot_changed {
                self.reveal_workspace_selection();
            }
            return;
        }
        if unavailable && !self.metadata_observers.is_empty() {
            self.metadata_observers.clear();
            self.presentation.invalidate();
        }
        if snapshot_changed {
            self.cancel_terminal_scroll();
            self.reveal_workspace_selection();
            self.presentation.invalidate();
            let picker_visible = self.workspace_model.directory_picker_visible();
            let was_focused = terminal_focused(
                self.window_focused,
                effective_workspace_focus(picker_was_visible, self.workspace_focus),
            );
            let is_focused = terminal_focused(
                self.window_focused,
                effective_workspace_focus(picker_visible, self.workspace_focus),
            );
            if was_focused != is_focused {
                let message = self.input.terminal_focus(is_focused);
                if was_focused {
                    self.send(message);
                }
            }
            if let Some((endpoint, live)) = self.workspace_model.active_attachment()
                && (self.active_endpoint.as_deref() != Some(endpoint)
                    || self.active_endpoint_live != live)
            {
                self.set_orbit_attachment(endpoint.to_vec(), live);
            }
            self.send_resize();
        }
        if received_snapshot {
            self.reconcile_metadata_observers();
        }
        if view_changed {
            self.refresh_client_view();
        }
    }

    fn reconcile_metadata_observers(&mut self) {
        let endpoints = self
            .workspace_model
            .snapshot()
            .map_or_else(HashSet::new, |snapshot| {
                visible_metadata_endpoints(
                    snapshot,
                    self.active_endpoint.as_deref(),
                    self.model.is_attached(),
                )
            });
        let before = self.metadata_observers.len();
        self.metadata_observers
            .retain(|endpoint, _| endpoints.contains(endpoint));
        let retained = self.metadata_observers.len();
        for endpoint in endpoints {
            let socket = PathBuf::from(OsString::from_vec(endpoint.clone()));
            let proxy = self.proxy.clone();
            self.metadata_observers
                .entry(endpoint)
                .or_insert_with(|| MetadataObserver {
                    transport: MetadataTransport::start(socket, move || {
                        let _ = proxy.send_event(UserEvent::Metadata);
                    }),
                    metadata: PaneMetadata::Connecting,
                });
        }
        if before != retained || retained != self.metadata_observers.len() {
            self.presentation.invalidate();
        }
    }

    fn handle_metadata(&mut self) {
        let mut changed = false;
        for observer in self.metadata_observers.values_mut() {
            let Some(event) = observer.transport.drain_event() else {
                continue;
            };
            let metadata = match event {
                MetadataEvent::Metadata(metadata) => PaneMetadata::Available {
                    working_directory: metadata.working_directory,
                },
                MetadataEvent::Unavailable => PaneMetadata::Unavailable,
            };
            if observer.metadata != metadata {
                observer.metadata = metadata;
                changed = true;
            }
        }
        if changed {
            self.presentation.invalidate();
            self.refresh_client_view();
        }
    }

    fn send_workspace(&mut self, action: WorkspaceAction) -> bool {
        self.presentation.has_presented_geometry() && self.queue_workspace(action)
    }

    fn queue_workspace(&mut self, action: WorkspaceAction) -> bool {
        let Some(transport) = &self.workspace_transport else {
            return false;
        };
        match transport.send(action) {
            Ok(()) => true,
            Err(error) => {
                if self
                    .workspace_model
                    .mark_unavailable(format!("Cannot queue Eon workspace action: {error}"))
                {
                    self.refresh_client_view();
                }
                false
            }
        }
    }

    fn handle_workspace_key(&mut self, event: &winit::event::KeyEvent) -> bool {
        let Some(snapshot) = self.workspace_model.snapshot() else {
            return false;
        };
        let picker_visible = directory_picker_visible(snapshot);
        let code = match event.physical_key {
            PhysicalKey::Code(code) => code,
            PhysicalKey::Unidentified(_) => {
                return !picker_visible && self.workspace_focus != WorkspaceFocus::Terminal;
            }
        };
        if code == KeyCode::F6 && !picker_visible {
            if event.state == ElementState::Pressed && !event.repeat {
                let focus = match self.workspace_focus {
                    WorkspaceFocus::Terminal => WorkspaceFocus::Tabs,
                    WorkspaceFocus::Tabs => WorkspaceFocus::Panes,
                    WorkspaceFocus::Panes => WorkspaceFocus::Terminal,
                };
                self.set_workspace_focus(focus);
            }
            return true;
        }
        let shortcut = workspace_shortcut(code, self.input.modifiers(), &snapshot.active_tab);
        let returns_to_terminal = !picker_visible
            && self.workspace_focus != WorkspaceFocus::Terminal
            && code == KeyCode::Escape;
        if self.input.consumes_host_shortcut(
            event.physical_key,
            event.state,
            event.repeat,
            shortcut.as_ref().is_some_and(|action| {
                workspace_shortcut_is_sendable(action, picker_visible)
                    || matches!(
                        action,
                        WorkspaceAction::PickTabDirectory
                            | WorkspaceAction::Move(_)
                            | WorkspaceAction::CloseTab { .. }
                    )
            }) || returns_to_terminal,
        ) {
            if returns_to_terminal && event.state == ElementState::Pressed {
                self.set_workspace_focus(WorkspaceFocus::Terminal);
            } else if let Some(action) = shortcut.filter(|action| {
                sends_workspace_shortcut(action, picker_visible, event.state, event.repeat)
            }) {
                self.send_workspace(action);
            }
            return true;
        }
        if picker_visible {
            return false;
        }
        if self.workspace_focus == WorkspaceFocus::Terminal {
            return false;
        }
        if event.state == ElementState::Pressed {
            let direction = match (self.workspace_focus, code) {
                (WorkspaceFocus::Tabs, KeyCode::ArrowLeft) => Some(WorkspaceDirection::Left),
                (WorkspaceFocus::Tabs, KeyCode::ArrowRight) => Some(WorkspaceDirection::Right),
                (WorkspaceFocus::Panes, KeyCode::ArrowUp) => Some(WorkspaceDirection::Up),
                (WorkspaceFocus::Panes, KeyCode::ArrowDown) => Some(WorkspaceDirection::Down),
                _ => None,
            };
            if let Some(direction) = direction {
                self.send_workspace(WorkspaceAction::Focus(direction));
            }
        }
        true
    }

    fn scroll_workspace(&mut self, delta: MouseScrollDelta, tabs: bool, metrics: CellMetrics) {
        let (horizontal, vertical) = match delta {
            MouseScrollDelta::LineDelta(x, y) => {
                (x * metrics.width * 3.0, y * metrics.height * 3.0)
            }
            MouseScrollDelta::PixelDelta(position) => (position.x as f32, position.y as f32),
        };
        if !horizontal.is_finite() || !vertical.is_finite() {
            return;
        }
        let Some(scene) = self.workspace_scene() else {
            return;
        };
        self.input.reset_scroll();
        let changed = if tabs {
            let movement = if horizontal == 0.0 {
                vertical
            } else {
                horizontal
            };
            let next = (scene.tab_scroll() - movement).clamp(0.0, scene.tab_scroll_limit());
            let changed = self.tab_scroll != next;
            self.tab_scroll = next;
            changed
        } else {
            let next = (scene.pane_scroll() - vertical).clamp(0.0, scene.pane_scroll_limit());
            let changed = self.pane_scroll != next;
            self.pane_scroll = next;
            changed
        };
        if changed {
            self.presentation.invalidate();
            self.refresh_client_view();
        }
    }

    fn handle_transport(&mut self, event: TransportEvent) {
        let retryable_busy = managed_busy_is_retryable(self.supervised, &event);
        let retryable_event = retryable_busy || matches!(&event, TransportEvent::RetryableLoss(_));
        let mut schedule_retry = false;
        match event {
            TransportEvent::Server(ServerMessage::Busy) if retryable_busy => {
                schedule_retry = apply_retryable_loss(
                    &mut self.model,
                    "Orbit already has a departing presentation client; retrying".into(),
                    self.retry_suppressed,
                );
            }
            TransportEvent::Server(message) => {
                let selection_finished = match &message {
                    ServerMessage::SelectionFinished { frame_revision } => Some(*frame_revision),
                    _ => None,
                };
                let preview_response = match &message {
                    ServerMessage::VerticalPreview(preview) => {
                        Some((preview.frame_revision, preview.direction))
                    }
                    _ => None,
                };
                let batch_response = match &message {
                    ServerMessage::ScrollOutcome(ScrollOutcome::Viewport {
                        requested_rows,
                        applied_rows,
                        ..
                    }) => Some((*requested_rows, *applied_rows)),
                    ServerMessage::ScrollOutcome(ScrollOutcome::TerminalOwned {
                        requested_rows,
                    }) => Some((*requested_rows, 0)),
                    _ => None,
                };
                let terminal_owned_batch = matches!(
                    &message,
                    ServerMessage::ScrollOutcome(ScrollOutcome::TerminalOwned { .. })
                );
                let server_failure = matches!(&message, ServerMessage::Failure(_));
                let plain_frame = matches!(&message, ServerMessage::Frame(_));
                if server_failure_suppresses_retry(&message) {
                    self.retry_suppressed = true;
                }
                let was_attached = self.model.is_attached();
                let previous_geometry = self
                    .model
                    .scene()
                    .map(|scene| (scene.columns, scene.rows, scene.screen));
                let frame = matches!(
                    &message,
                    ServerMessage::Frame(_)
                        | ServerMessage::WheelOutcome(WheelOutcome::Viewport { .. })
                        | ServerMessage::ScrollOutcome(ScrollOutcome::Viewport { .. })
                );
                let cell_height = self
                    .window
                    .as_ref()
                    .map_or(0.0, |state| f64::from(state.renderer.metrics().height));
                let batch_accepted = batch_response.is_none_or(|(requested, applied)| {
                    self.terminal_scroll
                        .accept_batch(requested, applied, cell_height)
                });
                let result = if batch_accepted {
                    self.model.apply(message)
                } else {
                    Err(ModelError::UnexpectedMessage)
                };
                let accepted_frame = frame && result.is_ok();
                let accepted = result.is_ok();
                match result {
                    Ok(Some(effect)) => self.write_clipboard(effect),
                    Ok(None) => {}
                    Err(error) => self.model.mark_lost(error.to_string()),
                }
                if accepted {
                    if let Some(frame_revision) = selection_finished {
                        self.selection_gate.finish_received(frame_revision);
                        if let Some(presented_revision) = self.presented_revision() {
                            self.flush_deferred_selection(presented_revision);
                        }
                    }
                    if server_failure {
                        self.terminal_scroll.reset();
                    } else if let Some((revision, direction)) = preview_response {
                        self.terminal_scroll.preview_arrived(revision, direction);
                    }
                    if terminal_owned_batch {
                        self.cancel_terminal_scroll();
                    } else if plain_frame {
                        self.terminal_scroll.rebase();
                    }
                }
                if server_failure
                    && (self.has_pointer_sequence()
                        || !matches!(self.selection_gate, SelectionGate::Ready))
                {
                    self.cancel_pointer_sequence();
                    self.selection_gate = SelectionGate::Ready;
                }
                if accepted_frame {
                    let geometry = self
                        .model
                        .scene()
                        .map(|scene| (scene.columns, scene.rows, scene.screen));
                    if plain_frame && geometry == previous_geometry {
                        // Output dirties the renderer, not the last presented input geometry.
                        self.presentation.content_changed();
                    } else {
                        self.presentation.invalidate();
                    }
                }
                if !was_attached && self.model.is_attached() {
                    self.reset_cursor_animation();
                    self.orbit_retry.reset();
                    self.deferred_selection.clear();
                    self.selection_gate = SelectionGate::Ready;
                    self.input.retire_orbit_generation();
                    self.send_resize();
                    if let Some(message) = self.input.latest_focus() {
                        self.send(message);
                    }
                    self.reconcile_metadata_observers();
                }
            }
            TransportEvent::Incompatible { version } => self.model.mark_incompatible(version),
            TransportEvent::InvalidInput(detail) => {
                self.terminal_scroll.reset();
                self.model.set_venus_notice(
                    LocalNoticeSource::Input,
                    format!("Venus could not encode input: {detail}"),
                );
            }
            TransportEvent::RetryableLoss(detail) => {
                schedule_retry =
                    apply_retryable_loss(&mut self.model, detail, self.retry_suppressed);
            }
            TransportEvent::Lost(detail) => self.model.mark_lost(detail),
        }
        if retryable_event || self.model.is_terminal() {
            self.terminal_scroll.reset();
            self.deferred_selection.clear();
            self.selection_gate = SelectionGate::Ready;
            self.input.retire_orbit_generation();
            self.reset_cursor_animation();
            self.transport = None;
            self.presentation.invalidate();
            if schedule_retry && self.retry_allowed() {
                self.orbit_retry.schedule(Instant::now());
            } else {
                self.orbit_retry.reset();
            }
        }
        self.refresh_client_view();
    }

    fn link_scene(&self) -> Option<(&Scene, PresentationIdentity)> {
        let workspace = self.workspace_scene();
        let identity = self.presentation_candidate(workspace.as_ref());
        (self.window_focused
            && !self.window_occluded
            && self.workspace_focus == WorkspaceFocus::Terminal
            && !self.workspace_model.directory_picker_visible()
            && self.workspace_model.notice().is_none()
            && !self.terminal_scroll.active()
            && self.links.unshifted
            && self.presentation.is_current(identity))
        .then(|| {
            (
                self.model.scene().expect("presented revision has a scene"),
                identity,
            )
        })
    }

    fn focused_link(&self) -> Option<Hyperlink<'_>> {
        let focus = self.links.focus?;
        let (scene, identity) = self.link_scene()?;
        (focus.identity == identity)
            .then(|| scene.hyperlink_at(focus.row, focus.column))
            .flatten()
    }

    fn link_viewport(&self) -> Option<(SceneRect, SceneRect)> {
        let state = self.window.as_ref()?;
        if let Some(workspace) = self.workspace_scene() {
            Some((workspace.terminal, workspace.visible_terminal()?))
        } else {
            let rect = SceneRect {
                width: state.renderer.size().width as f32,
                height: state.renderer.size().height as f32,
                ..SceneRect::default()
            };
            Some((rect, rect))
        }
    }

    fn pointer_link(&self) -> Option<LinkFocus> {
        if !self.links.pointer_inside || self.input.pointer_busy() {
            return None;
        }
        let (scene, identity) = self.link_scene()?;
        let state = self.window.as_ref()?;
        let workspace = self.workspace_scene();
        let (origin, visible) = self.link_viewport()?;
        let x = self.cursor.x as f32;
        let y = self.cursor.y as f32;
        if !(visible.left..visible.right()).contains(&x)
            || !(visible.top..visible.bottom()).contains(&y)
        {
            return None;
        }
        let metrics = state.renderer.metrics();
        if !self.status().is_empty() {
            let notice = state.renderer.notice_rect(workspace.as_ref());
            if (notice.left..notice.right()).contains(&x)
                && (notice.top..notice.bottom()).contains(&y)
            {
                return None;
            }
        }
        let column = ((x - origin.left - metrics.padding) / metrics.width).floor();
        let row = ((y - origin.top - metrics.padding) / metrics.height).floor();
        if column < 0.0
            || row < 0.0
            || column >= f32::from(scene.columns)
            || row >= f32::from(scene.rows)
        {
            return None;
        }
        let link = scene.hyperlink_at(row as u16, column as u16)?;
        Some(LinkFocus {
            identity,
            row: link.row,
            column: link.column,
        })
    }

    fn inspect_pointer_link(&mut self) {
        if self.links.keyboard || self.links.pressed.is_some() {
            return;
        }
        let focus = self.pointer_link();
        if self.links.focus != focus {
            self.links.focus = focus;
            self.links.page = 0;
            self.links.notice = None;
            self.refresh_client_view();
        }
    }

    fn link_status(&self) -> Option<String> {
        if let Some(notice) = &self.links.notice {
            return Some(notice.clone());
        }
        let link = self.focused_link()?;
        if link.uri.len() > MAX_LINK_BYTES {
            return Some("Link target exceeds 4096 bytes; opening and copying are disabled. Escape closes inspection.".into());
        }
        let target = escaped_link(link.uri);
        let width = self.link_page_width();
        let pages = target.len().div_ceil(width).max(1);
        let page = self.links.page.min(pages - 1);
        Some(format!(
            "{}\nLink {}/{} · Ctrl+Shift+O inspect · Tab · ←/→ · Enter open · Ctrl+Shift+C copy · Esc",
            link_page(&target, page, width),
            page + 1,
            pages
        ))
    }

    fn link_page_width(&self) -> usize {
        self.window.as_ref().map_or(40, |state| {
            ((state.renderer.size().width as f32 / state.renderer.metrics().width) as usize)
                .saturating_sub(22)
                .max(1)
        })
    }

    fn handle_link_key(&mut self, key: PhysicalKey, state: ElementState, repeat: bool) -> bool {
        let modifiers = self.input.modifiers();
        let inspect = key == PhysicalKey::Code(KeyCode::KeyO)
            && modifiers == session::Modifiers::CTRL.union(session::Modifiers::SHIFT)
            && self.workspace_focus == WorkspaceFocus::Terminal
            && !self.workspace_model.directory_picker_visible()
            && !self.input.pointer_busy();
        if !self
            .input
            .consumes_host_shortcut(key, state, repeat, inspect || self.links.keyboard)
        {
            return false;
        }
        if !shortcut_is_ready(state, repeat) {
            return true;
        }
        let PhysicalKey::Code(code) = key else {
            return true;
        };
        if code == KeyCode::Escape {
            self.links.keyboard = false;
            self.links.focus = None;
            self.links.notice = None;
        } else if inspect || code == KeyCode::Tab {
            self.links.keyboard = true;
            self.links.notice = None;
            let links = self
                .link_scene()
                .zip(self.link_viewport())
                .map(|((scene, identity), (origin, clip))| {
                    let metrics = self
                        .window
                        .as_ref()
                        .expect("link viewport has a window")
                        .renderer
                        .metrics();
                    scene
                        .hyperlinks()
                        .filter(|link| link.rect(origin, metrics).intersection(clip).is_some())
                        .map(|link| LinkFocus {
                            identity,
                            row: link.row,
                            column: link.column,
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let current = links
                .iter()
                .position(|link| Some(*link) == self.links.focus);
            let next = match (
                current,
                modifiers.contains(session::Modifiers::SHIFT) && !inspect,
            ) {
                (Some(index), true) => index
                    .checked_sub(1)
                    .unwrap_or(links.len().saturating_sub(1)),
                (Some(index), false) if !inspect => (index + 1) % links.len(),
                (Some(index), _) => index,
                (None, _) => 0,
            };
            self.links.focus = links.get(next).copied();
            self.links.page = 0;
            if self.links.focus.is_none() {
                self.links.notice =
                    Some("No links in the presented terminal. Escape returns to typing.".into());
            }
        } else if code == KeyCode::ArrowLeft || code == KeyCode::ArrowRight {
            self.links.notice = None;
            self.links.page = if code == KeyCode::ArrowLeft {
                self.links.page.saturating_sub(1)
            } else {
                self.focused_link().map_or(0, |link| {
                    (self.links.page + 1).min(
                        escaped_link(link.uri)
                            .len()
                            .div_ceil(self.link_page_width())
                            .saturating_sub(1),
                    )
                })
            };
        } else if code == KeyCode::Enter || code == KeyCode::NumpadEnter {
            self.activate_link(false);
        } else if code == KeyCode::KeyC
            && modifiers == session::Modifiers::CTRL.union(session::Modifiers::SHIFT)
        {
            self.activate_link(true);
        }
        self.refresh_client_view();
        true
    }

    fn activate_link(&mut self, copy: bool) {
        let uri = self
            .focused_link()
            .filter(|_| self.links.focus == self.links.presented)
            .map(|link| link.uri.to_owned());
        let result = match uri {
            None => Err("Link changed or has not been presented. Inspect it again."),
            Some(uri) if copy => validate_copy_uri(&uri).and_then(|()| {
                write_native_clipboard(wl_clipboard_rs::copy::ClipboardType::Regular, uri)
                    .map(|()| "Link copied.")
                    .map_err(|_| "Could not copy the link to the native clipboard.")
            }),
            Some(_) if self.link_opener.is_some() => Err("A link dispatch is already pending."),
            Some(uri) => LinkOpener::start(&uri).map(|opener| {
                self.link_opener = Some(opener);
                "Opening link…"
            }),
        };
        self.links.notice = Some(result.unwrap_or_else(|error| error).into());
    }

    fn handle_link_button(&mut self, state: ElementState, button: MouseButton) -> bool {
        if button != MouseButton::Left {
            return false;
        }
        if state == ElementState::Released
            && let Some(pressed) = self.links.pressed.take()
        {
            if self.pointer_link() == Some(pressed) {
                self.links.focus = Some(pressed);
                self.activate_link(false);
            }
            self.refresh_client_view();
            return true;
        }
        if state == ElementState::Pressed
            && self.input.modifiers() == session::Modifiers::CTRL.union(session::Modifiers::SHIFT)
            && let Some(focus) = self.pointer_link()
        {
            self.links.keyboard = false;
            self.links.focus = Some(focus);
            self.links.pressed = Some(focus);
            self.refresh_client_view();
            return true;
        }
        false
    }

    fn refresh_client_view(&mut self) {
        if self.links.focus.is_some() && self.focused_link().is_none() {
            self.links.focus = None;
            if self.links.keyboard {
                self.links.notice = Some(
                    "Links changed. Tab inspects the current frame; Escape returns to typing."
                        .into(),
                );
            }
        }
        let status = self.status();
        let workspace = self.workspace_scene();
        let scrollback_label = self.scrollback_label(workspace.as_ref());
        let workspace_focus = effective_workspace_focus(
            self.workspace_model.directory_picker_visible(),
            self.workspace_focus,
        );
        let ime_allowed = !self.links.keyboard
            && ime_allowed(
                self.window_focused,
                workspace_focus,
                self.model.is_attached(),
            );
        let Some(state) = &mut self.window else {
            return;
        };
        state.window.set_title(window_title(
            self.model.scene().map(|scene| scene.title.as_str()),
            self.render_notice.as_deref(),
        ));
        if let Some(scene) = self.model.scene()
            && let Some(cursor) = scene.cursor
        {
            let metrics = state.renderer.metrics();
            let origin = workspace.as_ref().map_or((0.0, 0.0), |workspace| {
                (workspace.terminal.left, workspace.terminal.top)
            });
            let left =
                origin.0 + metrics.padding + f32::from(cursor.leading_column()) * metrics.width;
            let top = origin.1 + metrics.padding + f32::from(cursor.row) * metrics.height;
            state.window.set_ime_cursor_area(
                PhysicalPosition::new(f64::from(left), f64::from(top)),
                PhysicalSize::new(f64::from(metrics.width * 2.0), f64::from(metrics.height)),
            );
        }
        state.window.set_ime_allowed(ime_allowed);
        state.window.set_cursor(if self.links.focus.is_some() {
            CursorIcon::Pointer
        } else {
            CursorIcon::Default
        });
        state.accessibility.update(
            &mut state.adapter,
            self.model.scene(),
            workspace.as_ref(),
            workspace_focus,
            &status,
            scrollback_label.as_deref(),
            state.renderer.size(),
            state.renderer.metrics(),
        );
        state.window.request_redraw();
    }

    fn send(&mut self, message: ClientMessage) -> bool {
        let resize = match &message {
            ClientMessage::Resize(size) => Some(*size),
            _ => None,
        };
        if matches!(
            &message,
            ClientMessage::Mouse(_)
                | ClientMessage::Selection(
                    SelectionAction::Begin { .. }
                        | SelectionAction::Update { .. }
                        | SelectionAction::Finish { .. }
                )
        ) && let Some(size) = self.terminal_size().and_then(|screen| {
            self.window
                .as_ref()
                .and_then(|state| surface_size(screen, state.renderer.metrics()))
        }) && self.last_resize != Some(size)
            && (!self.send(ClientMessage::Resize(size)) || !can_follow_implicit_resize(&message))
        {
            return false;
        }
        let notice_source = if resize.is_some() {
            LocalNoticeSource::Resize
        } else {
            LocalNoticeSource::Input
        };
        if !self.model.is_attached() {
            return false;
        }
        let Some(transport) = &self.transport else {
            return false;
        };
        if let Err(error) = transport.send(message) {
            self.model
                .set_venus_notice(LocalNoticeSource::Queue, error.to_string());
            self.refresh_client_view();
            return false;
        }
        if let Some(size) = resize {
            self.last_resize = Some(size);
            self.deferred_selection.clear();
            self.input.cancel_selection();
        }
        let queue_recovered = self.model.clear_venus_notice(LocalNoticeSource::Queue);
        let clipboard_cleared = dismisses_clipboard_notice(notice_source)
            && self.model.clear_venus_notice(LocalNoticeSource::Clipboard);
        if self.model.clear_venus_notice(notice_source) || queue_recovered || clipboard_cleared {
            self.refresh_client_view();
        }
        true
    }

    fn has_pointer_sequence(&self) -> bool {
        self.input.is_selecting()
            || self.deferred_selection.iter().any(|message| {
                matches!(
                    message,
                    ClientMessage::Selection(
                        SelectionAction::Begin { .. }
                            | SelectionAction::Update { .. }
                            | SelectionAction::Finish { .. }
                    )
                )
            })
    }

    fn cancel_pointer_sequence(&mut self) {
        let cancel_server = pointer_sequence_needs_cancel(
            &self.selection_gate,
            self.input.is_selecting(),
            &self.deferred_selection,
        );
        self.deferred_selection.clear();
        self.input.cancel_selection();
        if cancel_server {
            self.send(ClientMessage::Selection(SelectionAction::Cancel));
        }
    }

    fn send_selection(&mut self, message: ClientMessage) {
        if self.send(message.clone()) {
            self.input.commit_selection(&message);
            if matches!(
                message,
                ClientMessage::Selection(SelectionAction::Finish { .. })
            ) {
                self.selection_gate.finish_sent();
            }
        } else {
            self.cancel_pointer_sequence();
        }
    }

    fn send_or_defer_selection(&mut self, message: ClientMessage, presentation_current: bool) {
        if selection_presentation_is_ready(&message, presentation_current)
            && self.deferred_selection.is_empty()
            && matches!(self.selection_gate, SelectionGate::Ready)
        {
            self.send_selection(message);
        } else if queue_deferred_selection(&mut self.deferred_selection, message.clone()) {
            self.input.commit_selection(&message);
        } else {
            self.cancel_pointer_sequence();
            self.model.set_venus_notice(
                LocalNoticeSource::Input,
                "Venus selection input exceeded its bounded capacity",
            );
            self.refresh_client_view();
        }
    }

    fn flush_deferred_selection(&mut self, frame_revision: u64) {
        if !self.selection_gate.admits(frame_revision) {
            return;
        }
        let batch = take_deferred_selection(&mut self.deferred_selection, frame_revision);
        let mut server_gesture = matches!(
            batch.first(),
            Some(ClientMessage::Selection(
                SelectionAction::Update { .. } | SelectionAction::Finish { .. }
            ))
        );
        for message in batch {
            let begins = matches!(
                message,
                ClientMessage::Selection(SelectionAction::Begin { .. })
            );
            let finished = matches!(
                message,
                ClientMessage::Selection(SelectionAction::Finish { .. })
            );
            if !self.send(message) {
                self.deferred_selection.clear();
                self.input.cancel_selection();
                if server_gesture {
                    self.send(ClientMessage::Selection(SelectionAction::Cancel));
                }
                return;
            }
            if finished {
                self.selection_gate.finish_sent();
            }
            server_gesture |= begins;
        }
    }

    fn presented_revision(&self) -> Option<u64> {
        self.presentation.presented_revision()
    }

    fn write_clipboard(&mut self, effect: ClipboardEffect) {
        let clipboard = wayland_clipboard_type(&effect);
        let text = match effect {
            ClipboardEffect::SelectionCopy { text, .. }
            | ClipboardEffect::TerminalWrite { text, .. } => text,
        };
        let result = write_native_clipboard(clipboard, text);
        self.model.set_venus_notice(
            LocalNoticeSource::Clipboard,
            clipboard_notice(clipboard, result),
        );
    }

    fn paste_clipboard(&mut self) {
        if !self.model.is_attached() {
            return;
        }
        let result = read_native_clipboard().and_then(clipboard_paste_message);
        match result {
            Ok(message) => {
                self.send(message);
            }
            Err(error) => {
                self.model
                    .set_venus_notice(LocalNoticeSource::Clipboard, clipboard_paste_notice(error));
                self.refresh_client_view();
            }
        }
    }

    fn send_resize(&mut self) {
        let Some(state) = &self.window else {
            return;
        };
        let Some(screen) = self.terminal_size() else {
            return;
        };
        let Some(size) = surface_size(screen, state.renderer.metrics()) else {
            self.model.set_venus_notice(
                LocalNoticeSource::Resize,
                "Window dimensions are outside Orbit's accepted surface range",
            );
            return;
        };
        if self.last_resize == Some(size) {
            self.model.clear_venus_notice(LocalNoticeSource::Resize);
            return;
        }
        self.send(ClientMessage::Resize(size));
    }

    fn status(&self) -> String {
        if let Some(notice) = &self.render_notice {
            return notice.clone();
        }
        if let Some(notice) = self.workspace_model.notice() {
            return notice.to_owned();
        }
        if self.model.is_attached()
            && !self.model.awaiting_current_frame()
            && let Some(status) = self.link_status()
        {
            return status;
        }
        if let Some(notice) = self.model.notice() {
            return notice.to_owned();
        }
        if self.initial_scale_pending {
            return "Opening terminal.".into();
        }
        if let Some(socket) = &self.workspace_socket
            && self.workspace_model.snapshot().is_none()
        {
            return format!("Connecting to Eon workspace at {}", socket.display());
        }
        match self.model.connection() {
            ConnectionState::Connecting | ConnectionState::Attached
                if self.workspace_model.snapshot().is_some() =>
            {
                String::new()
            }
            ConnectionState::Connecting => {
                format!(
                    "Connecting to Orbit at {}",
                    self.orbit_socket
                        .as_ref()
                        .expect("connecting Orbit has an attachment socket")
                        .display()
                )
            }
            ConnectionState::Attached if self.model.awaiting_current_frame() => {
                "Attached to Orbit. Waiting for its current frame.".into()
            }
            ConnectionState::Attached => String::new(),
            ConnectionState::Busy => "Orbit already has an attached presentation client.".into(),
            ConnectionState::Incompatible { version } => format!(
                "Orbit uses local-session revision {version}; Venus requires revision {}.",
                session::VERSION
            ),
            ConnectionState::Lost { detail } => format!("Orbit connection lost: {detail}"),
            ConnectionState::Exited { code } => {
                format!("The authoritative Orbit process exited with status {code}.")
            }
        }
    }

    fn render(&mut self) {
        if self.window_focused && !self.window_occluded {
            self.terminal_scroll.advance(Instant::now());
            self.drive_terminal_scroll();
        }
        if !cursor_animation_allowed(self.window_focused, self.window_occluded) {
            self.reset_cursor_animation();
        }
        let owned_status;
        let status = if let Some(notice) = self.render_notice.as_deref() {
            notice
        } else {
            owned_status = self.status();
            owned_status.as_str()
        };
        let preedit = self.input.preedit();
        let workspace = self.workspace_scene();
        let workspace_focus = effective_workspace_focus(
            self.workspace_model.directory_picker_visible(),
            self.workspace_focus,
        );
        let candidate = self.presentation_candidate(workspace.as_ref());
        let scroll_offset = self.window.as_ref().map_or(0.0, |state| {
            self.terminal_scroll.offset(
                self.model.scroll_preview(),
                f64::from(state.renderer.metrics().height),
            )
        });
        let kinetic_active = self.terminal_scroll.velocity != 0.0;
        let scrollback_label = self.scrollback_label(workspace.as_ref()).filter(|_| {
            workspace
                .as_ref()
                .is_some_and(|workspace| !workspace.directory_picker())
                || (!self.input.pointer_busy()
                    && self.links.focus.is_none()
                    && !self.links.keyboard
                    && !self.model.scene().is_some_and(Scene::has_selected_content))
        });
        let highlighted_link = self.focused_link().map(|link| (link.row, link.column));
        let hovered_header = (self.window_focused
            && self.links.pointer_inside
            && !self.input.pointer_busy()
            && !self.links.keyboard)
            .then(|| {
                workspace.as_ref().and_then(|scene| {
                    match scene.hit_test(self.cursor.x as f32, self.cursor.y as f32) {
                        Some(WorkspaceHit::Tab(id)) => Some((WorkspaceFocus::Tabs, id.to_owned())),
                        Some(WorkspaceHit::Pane(id)) => {
                            Some((WorkspaceFocus::Panes, id.to_owned()))
                        }
                        _ => None,
                    }
                })
            })
            .flatten();
        let mut refresh = false;
        let mut presented = false;
        let Some(state) = &mut self.window else {
            return;
        };
        state.renderer.set_hyperlink(highlighted_link);
        state
            .renderer
            .set_scrollback_label(scrollback_label.clone());
        state.renderer.set_hovered_header(hovered_header);
        match state.renderer.render(
            self.model.scene(),
            self.model.scroll_preview(),
            scroll_offset,
            workspace.as_ref(),
            workspace_focus,
            status,
            self.blink_visible,
            preedit,
            candidate.generation,
        ) {
            Ok(PresentOutcome::Presented) => {
                presented = true;
                refresh = self.render_notice.take().is_some();
                self.presentation.publish(candidate);
                self.links.unshifted = scroll_offset == 0.0;
                self.links.presented = self.links.focus;
                if kinetic_active {
                    state.window.request_redraw();
                }
                if let Some(revision) = candidate.revision {
                    self.flush_deferred_selection(revision);
                }
            }
            Ok(PresentOutcome::Deferred) => {}
            Ok(PresentOutcome::Occluded) => {
                self.terminal_scroll.cancel();
                self.links.unshifted = false;
            }
            Ok(PresentOutcome::Recovered) => {
                self.links.unshifted = false;
                self.presentation.invalidate();
                state.window.request_redraw();
            }
            Err(error) => {
                self.terminal_scroll.cancel();
                if let Some(notice) = record_render_failure(
                    &mut self.render_notice,
                    &mut self.presentation,
                    error,
                    |title| state.window.set_title(title),
                    |message| report(message),
                ) {
                    state.accessibility.update(
                        &mut state.adapter,
                        self.model.scene(),
                        workspace.as_ref(),
                        workspace_focus,
                        notice,
                        scrollback_label.as_deref(),
                        state.renderer.size(),
                        state.renderer.metrics(),
                    );
                }
            }
        }
        if refresh {
            self.refresh_client_view();
        }
        if presented {
            self.inspect_pointer_link();
        }
    }
}

fn window_attributes(
    decorations: bool,
    background_opacity: f32,
    background_blur: bool,
) -> WindowAttributes {
    Window::default_attributes()
        .with_title("Venus")
        .with_inner_size(LogicalSize::new(960.0, 600.0))
        .with_decorations(decorations)
        .with_transparent(background_opacity < 1.0)
        .with_blur(background_blur)
}

impl ApplicationHandler<UserEvent> for Application {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none()
            && self.initial_scale_pending
            && self.workspace_model.snapshot().is_none()
            && let Some(socket) = self.workspace_socket.clone()
        {
            if self.workspace_transport.is_none() {
                self.start_workspace(socket);
            }
            return;
        }
        if self.window.is_none()
            && let Err(error) = self.create_window(event_loop)
        {
            self.fatal_error = Some(error.to_string());
            event_loop.exit();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let workspace = self.workspace_scene();
        let workspace_focus = effective_workspace_focus(
            self.workspace_model.directory_picker_visible(),
            self.workspace_focus,
        );
        let candidate = self.presentation_candidate(workspace.as_ref());
        let presentation_current = self.presentation.has_presented_geometry();
        let presented_revision = self.presentation.presented_revision();
        let Some(state) = &mut self.window else {
            return;
        };
        if state.window.id() != window_id {
            return;
        }
        state.adapter.process_event(&state.window, &event);

        match event {
            WindowEvent::CloseRequested | WindowEvent::Destroyed => event_loop.exit(),
            WindowEvent::Resized(size) => {
                self.next_animation = None;
                self.input.reset_scroll();
                self.terminal_scroll.cancel();
                state.renderer.resize(size, state.scale_factor);
                self.reveal_workspace_selection();
                self.presentation.invalidate();
                self.send_resize();
                self.refresh_client_view();
            }
            WindowEvent::ScaleFactorChanged {
                scale_factor,
                mut inner_size_writer,
            } => {
                if self.initial_scale_pending {
                    let initial = initial_window_size(
                        self.columns,
                        self.rows,
                        state.renderer.metrics_at_scale(scale_factor),
                        scale_factor,
                        self.workspace_model.snapshot(),
                    );
                    match initial.and_then(|size| {
                        inner_size_writer
                            .request_inner_size(size)
                            .map_err(Into::into)
                    }) {
                        Ok(()) => {}
                        Err(error) => {
                            self.fatal_error = Some(error.to_string());
                            event_loop.exit();
                            return;
                        }
                    }
                }
                self.terminal_scroll.cancel();
                state.scale_factor = scale_factor;
                self.presentation.invalidate();
                self.cancel_pointer_sequence();
            }
            WindowEvent::Occluded(occluded) => {
                self.window_occluded = occluded;
                self.next_animation = None;
                self.terminal_scroll.cancel();
                state.renderer.reset_cursor_animation();
                if occluded {
                    self.presentation.invalidate();
                    self.cancel_pointer_sequence();
                } else {
                    state.window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => self.render(),
            WindowEvent::ModifiersChanged(modifiers) => {
                self.cancel_terminal_scroll();
                self.input.set_modifiers(modifiers.state());
                self.inspect_pointer_link();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                self.cancel_terminal_scroll();
                if self
                    .input
                    .suppresses_retired_key(event.physical_key, event.state, event.repeat)
                    || self.handle_link_key(event.physical_key, event.state, event.repeat)
                    || self.handle_workspace_key(&event)
                {
                } else if self.input.consumes_paste_shortcut(
                    &event.key_without_modifiers(),
                    event.physical_key,
                    event.state,
                    event.repeat,
                ) {
                    if shortcut_is_ready(event.state, event.repeat) {
                        self.paste_clipboard();
                    }
                } else if self.input.consumes_copy_shortcut(
                    event.physical_key,
                    event.state,
                    event.repeat,
                ) {
                    if copy_is_ready(self.input.is_selecting(), event.state, event.repeat) {
                        self.send_or_defer_selection(
                            ClientMessage::Selection(SelectionAction::Copy),
                            presentation_current,
                        );
                    }
                } else if let Some(message) = self.input.key(&event)
                    && (self.send(message) || event.state == ElementState::Released)
                {
                    self.input.commit_key(event.physical_key, event.state);
                }
            }
            WindowEvent::Ime(event) => {
                self.cancel_terminal_scroll();
                let allowed = !self.links.keyboard
                    && ime_allowed(
                        self.window_focused,
                        workspace_focus,
                        self.model.is_attached(),
                    );
                if ime_reaches_terminal(allowed, &event)
                    && let Some(message) =
                        native_ime_message(&mut self.input, &mut self.model, event)
                {
                    self.send(message);
                }
                self.refresh_client_view();
            }
            WindowEvent::Focused(focused) => {
                self.window_focused = focused;
                self.next_animation = None;
                self.terminal_scroll.cancel();
                state.renderer.reset_cursor_animation();
                if !focused {
                    self.links.keyboard = false;
                    self.links.focus = None;
                    self.links.pressed = None;
                    self.cancel_pointer_sequence();
                }
                let message = self
                    .input
                    .native_focus(focused, terminal_focused(focused, workspace_focus));
                self.send(message);
                self.refresh_client_view();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let header_at = |position: PhysicalPosition<f64>| {
                    workspace.as_ref().and_then(|scene| {
                        match scene.hit_test(position.x as f32, position.y as f32) {
                            hit @ Some(WorkspaceHit::Tab(_) | WorkspaceHit::Pane(_)) => hit,
                            _ => None,
                        }
                    })
                };
                if header_at(self.cursor) != header_at(position)
                    || (!self.links.pointer_inside && header_at(position).is_some())
                {
                    state.window.request_redraw();
                }
                self.cursor = position;
                self.links.pointer_inside = true;
                if self.links.pressed.is_some() {
                    return;
                }
                let motion = move_terminal_pointer(&mut self.input, position, workspace.as_ref());
                if self.input.is_selecting() {
                    let screen = terminal_screen(workspace.as_ref(), state.renderer.size());
                    let size = surface_size(screen, state.renderer.metrics());
                    if let Some(message) = size.and_then(|size| self.input.selection_motion(size)) {
                        self.send_or_defer_selection(message, presentation_current);
                    }
                } else if presented_revision.is_some()
                    && workspace.as_ref().is_none_or(|workspace| {
                        matches!(
                            workspace.hit_test(position.x as f32, position.y as f32),
                            Some(WorkspaceHit::Terminal)
                        )
                    })
                    && let Some(message) = motion
                {
                    self.send(message);
                }
                self.inspect_pointer_link();
            }
            WindowEvent::CursorLeft { .. } => {
                self.links.pointer_inside = false;
                state.window.request_redraw();
                self.cancel_terminal_scroll();
                self.cancel_pointer_sequence();
                if !self.links.keyboard {
                    self.links.focus = None;
                    self.refresh_client_view();
                }
            }
            WindowEvent::MouseInput {
                state: button_state,
                button,
                ..
            } => {
                if button_state == ElementState::Pressed {
                    self.terminal_scroll.cancel();
                    state.renderer.reset_cursor_animation();
                    state.window.request_redraw();
                }
                let renderer_size = state.renderer.size();
                let metrics = state.renderer.metrics();
                if self.handle_link_button(button_state, button) {
                    return;
                }
                if button_state == ElementState::Pressed {
                    self.links.keyboard = false;
                    self.links.focus = None;
                    self.links.notice = None;
                    self.refresh_client_view();
                }
                let hit = workspace.as_ref().and_then(|workspace| {
                    workspace.hit_test(self.cursor.x as f32, self.cursor.y as f32)
                });
                if presentation_current
                    && !workspace
                        .as_ref()
                        .is_some_and(|scene| scene.directory_picker())
                    && button_state == ElementState::Pressed
                    && button == MouseButton::Left
                {
                    let target = match hit {
                        Some(WorkspaceHit::Tab(id)) => Some((WorkspaceFocus::Tabs, id.to_owned())),
                        Some(WorkspaceHit::Pane(id)) => {
                            Some((WorkspaceFocus::Panes, id.to_owned()))
                        }
                        Some(WorkspaceHit::Terminal) => {
                            self.set_workspace_focus(WorkspaceFocus::Terminal);
                            None
                        }
                        None => None,
                    };
                    if let Some((focus, id)) = target {
                        if self.send_workspace(WorkspaceAction::FocusId(id)) {
                            self.set_workspace_focus(focus);
                        }
                        return;
                    }
                }
                if !button_reaches_terminal(
                    workspace.is_some(),
                    matches!(hit, Some(WorkspaceHit::Terminal)),
                    button_state,
                    presentation_current
                        || button == MouseButton::Left && candidate.revision.is_some(),
                ) {
                    return;
                }
                let _ = move_terminal_pointer(&mut self.input, self.cursor, workspace.as_ref());
                let screen = terminal_screen(workspace.as_ref(), renderer_size);
                let size = surface_size(screen, metrics);
                let selection = size.and_then(|size| {
                    self.input.selection_button(
                        button_state,
                        button,
                        size,
                        presented_revision.or(candidate.revision),
                        u64::try_from(self.input_epoch.elapsed().as_nanos()).unwrap_or(u64::MAX),
                    )
                });
                if let Some(message) = selection {
                    self.send_or_defer_selection(message, presentation_current);
                } else if let Some(message) =
                    self.input
                        .mouse_button(button_state, button, presented_revision.is_some())
                    && (self.send(message) || button_state == ElementState::Released)
                {
                    self.input.commit_mouse_button(button_state, button);
                }
            }
            WindowEvent::MouseWheel { delta, phase, .. } => {
                if self.input.is_selecting() {
                    return;
                }
                let metrics = state.renderer.metrics();
                let hit = workspace.as_ref().and_then(|workspace| {
                    workspace.hit_test(self.cursor.x as f32, self.cursor.y as f32)
                });
                if workspace
                    .as_ref()
                    .is_some_and(|scene| scene.directory_picker())
                    && !matches!(hit, Some(WorkspaceHit::Terminal))
                {
                    self.terminal_scroll.cancel();
                    return;
                }
                if !matches!(hit, Some(WorkspaceHit::Terminal)) && workspace.is_some() {
                    self.terminal_scroll.cancel();
                    state.renderer.reset_cursor_animation();
                    state.window.request_redraw();
                }
                if workspace.as_ref().is_some_and(|scene| {
                    scene
                        .tab_viewport
                        .contains(self.cursor.x as f32, self.cursor.y as f32)
                }) {
                    if presentation_current {
                        self.scroll_workspace(delta, true, metrics);
                    }
                    return;
                }
                if matches!(hit, Some(WorkspaceHit::Pane(_))) {
                    if presentation_current {
                        self.scroll_workspace(delta, false, metrics);
                    }
                    return;
                }
                if workspace.is_some() && !matches!(hit, Some(WorkspaceHit::Terminal)) {
                    return;
                }
                if presented_revision.is_none() {
                    return;
                }
                let _ = move_terminal_pointer(&mut self.input, self.cursor, workspace.as_ref());
                let (horizontal, vertical) = match delta {
                    MouseScrollDelta::LineDelta(horizontal, vertical) => (
                        MouseScrollDelta::LineDelta(horizontal, 0.0),
                        f64::from(vertical),
                    ),
                    MouseScrollDelta::PixelDelta(position) => (
                        MouseScrollDelta::PixelDelta(PhysicalPosition::new(position.x, 0.0)),
                        position.y,
                    ),
                };
                for message in self.input.wheel(horizontal, metrics.width, metrics.height) {
                    if !self.send(message) {
                        break;
                    }
                }
                match delta {
                    MouseScrollDelta::LineDelta(_, _) => self
                        .terminal_scroll
                        .push_lines(vertical, f64::from(metrics.height)),
                    MouseScrollDelta::PixelDelta(_) => self.terminal_scroll.push_pixels(
                        vertical,
                        phase,
                        Instant::now(),
                        f64::from(metrics.height),
                    ),
                }
                self.drive_terminal_scroll();
                if let Some(state) = &self.window {
                    state.window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Exit => event_loop.exit(),
            UserEvent::Present => {
                if !self.window_focused
                    && let Some(state) = &self.window
                {
                    state
                        .window
                        .request_user_attention(Some(UserAttentionType::Informational));
                }
            }
            UserEvent::Metadata => self.handle_metadata(),
            UserEvent::Transport => {
                let events = self
                    .transport
                    .as_ref()
                    .map_or_else(Vec::new, Transport::drain_events);
                for event in events {
                    self.handle_transport(event);
                }
                self.drive_terminal_scroll();
            }
            UserEvent::Workspace => {
                let events = self
                    .workspace_transport
                    .as_ref()
                    .map_or_else(Vec::new, WorkspaceTransport::drain_events);
                for event in events {
                    self.handle_workspace(event);
                }
                if self.window.is_none() {
                    if self.workspace_model.snapshot().is_some() {
                        self.resumed(event_loop);
                    } else if let Some(notice) = self.workspace_model.notice() {
                        self.fatal_error = Some(notice.to_owned());
                        event_loop.exit();
                    }
                }
            }
            UserEvent::AccessKit(event) => {
                let Some(state) = &mut self.window else {
                    return;
                };
                if state.window.id() != event.window_id {
                    return;
                }
                match event.window_event {
                    AccessKitWindowEvent::InitialTreeRequested => self.refresh_client_view(),
                    AccessKitWindowEvent::ActionRequested(request)
                        if matches!(
                            request.action,
                            AccessibilityAction::Click | AccessibilityAction::Focus
                        ) =>
                    {
                        let target = state.accessibility.workspace_target(request.target_node);
                        if let Some(focus) = target.and_then(|target| {
                            accessibility_workspace_focus(target, |action| {
                                self.queue_workspace(action)
                            })
                        }) {
                            self.set_workspace_focus(focus);
                        }
                    }
                    AccessKitWindowEvent::ActionRequested(_)
                    | AccessKitWindowEvent::AccessibilityDeactivated => {}
                }
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Initial Wayland scale can arrive after the first buffer. Once the
        // window has entered an output, later scale/size changes belong to the user.
        if self.initial_scale_pending
            && self.fatal_error.is_none()
            && self
                .window
                .as_ref()
                .is_some_and(|state| state.window.current_monitor().is_some())
        {
            self.initial_scale_pending = false;
            if self.startup_admission
                && self
                    .terminal_size()
                    .and_then(|size| {
                        surface_size(size, self.window.as_ref().unwrap().renderer.metrics())
                    })
                    .is_none()
            {
                self.fatal_error =
                    Some("initial native window leaves no valid terminal grid".into());
                event_loop.exit();
                return;
            }
            if self.startup_admission
                && let Err(error) = io::stdout()
                    .write_all(b"ready-v1")
                    .and_then(|()| io::stdout().flush())
            {
                self.fatal_error = Some(format!("Cannot report Venus startup readiness: {error}"));
                event_loop.exit();
                return;
            }
            self.start_initial_attachment();
            self.refresh_client_view();
        }
        let now = Instant::now();
        if let Some(notice) = self
            .link_opener
            .as_mut()
            .and_then(|opener| opener.poll(now))
        {
            self.link_opener = None;
            self.links.notice = Some(notice.into());
            self.refresh_client_view();
        }
        self.retry_orbit(now);
        let blinking = self
            .model
            .scene()
            .is_some_and(|scene| scene.has_blinking_content());
        if update_blink(blinking, &mut self.blink_visible, &mut self.next_blink, now)
            && let Some(state) = &self.window
        {
            state.window.request_redraw();
        }
        let animation_active = self
            .window
            .as_ref()
            .is_some_and(|state| state.renderer.cursor_animation_active());
        if update_animation_deadline(
            animation_active,
            cursor_animation_allowed(self.window_focused, self.window_occluded),
            &mut self.next_animation,
            now,
        ) && let Some(state) = &self.window
        {
            state.window.request_redraw();
        }
        event_loop.set_control_flow(
            [
                self.next_blink,
                self.next_animation,
                self.orbit_retry.deadline,
                // winit may suppress output-enter wakeups. Poll only until
                // this window completes its initial native size admission.
                (self.initial_scale_pending && self.window.is_some())
                    .then_some(now + ANIMATION_FRAME_INTERVAL),
                self.link_opener
                    .as_ref()
                    .map(|_| now + Duration::from_millis(50)),
            ]
            .into_iter()
            .flatten()
            .min()
            .map_or(ControlFlow::Wait, ControlFlow::WaitUntil),
        );
    }
}

fn update_blink(
    blinking: bool,
    visible: &mut bool,
    deadline: &mut Option<Instant>,
    now: Instant,
) -> bool {
    if !blinking {
        *visible = true;
        *deadline = None;
    } else if deadline.is_none() {
        *deadline = Some(now + BLINK_INTERVAL);
    } else if deadline.is_some_and(|next| now >= next) {
        *visible = !*visible;
        *deadline = Some(now + BLINK_INTERVAL);
        return true;
    }
    false
}

fn cursor_animation_allowed(focused: bool, occluded: bool) -> bool {
    focused && !occluded
}

fn update_animation_deadline(
    active: bool,
    allowed: bool,
    deadline: &mut Option<Instant>,
    now: Instant,
) -> bool {
    if !active || !allowed {
        *deadline = None;
    } else if deadline.is_none() {
        *deadline = Some(now + ANIMATION_FRAME_INTERVAL);
    } else if deadline.is_some_and(|next| now >= next) {
        *deadline = Some(now + ANIMATION_FRAME_INTERVAL);
        return true;
    }
    false
}

fn retry_is_allowed(
    workspace: bool,
    selected: Option<(&[u8], bool)>,
    active: Option<&[u8]>,
) -> bool {
    !workspace || selected.is_some_and(|(endpoint, live)| live && active == Some(endpoint))
}

fn visible_metadata_endpoints(
    snapshot: &eon_workspace_protocol::v4::Snapshot,
    selected_endpoint: Option<&[u8]>,
    selected_attached: bool,
) -> HashSet<Vec<u8>> {
    if directory_picker_visible(snapshot) {
        return HashSet::new();
    }
    let mut endpoints = snapshot
        .tabs
        .iter()
        .find(|tab| tab.id == snapshot.active_tab)
        .into_iter()
        .flat_map(|tab| &tab.panes)
        .filter(|pane| pane.live)
        .map(|pane| pane.endpoint.clone())
        .collect::<HashSet<_>>();
    if !selected_attached && let Some(endpoint) = selected_endpoint {
        endpoints.remove(endpoint);
    }
    endpoints
}

fn server_failure_suppresses_retry(message: &ServerMessage) -> bool {
    matches!(
        message,
        ServerMessage::Failure(failure)
            if matches!(failure.code, FailureCode::Protocol | FailureCode::Terminal)
    )
}

fn managed_busy_is_retryable(supervised: bool, event: &TransportEvent) -> bool {
    supervised && matches!(event, TransportEvent::Server(ServerMessage::Busy))
}

fn apply_retryable_loss(model: &mut SessionModel, detail: String, retry_suppressed: bool) -> bool {
    let schedule_retry = !model.is_terminal() && !retry_suppressed;
    model.mark_lost_preserving_constraining_notice(detail);
    schedule_retry
}

fn window_title<'a>(scene_title: Option<&'a str>, render_notice: Option<&'a str>) -> &'a str {
    render_notice
        .or_else(|| scene_title.filter(|title| !title.is_empty()))
        .unwrap_or("Venus")
}

fn report(message: impl std::fmt::Display) {
    let _ = writeln!(io::stderr().lock(), "venus: {message}");
}

fn record_render_failure<'a>(
    render_notice: &'a mut Option<String>,
    presentation: &mut PresentationState,
    error: impl std::fmt::Display,
    set_title: impl FnOnce(&str),
    report: impl FnOnce(&str),
) -> Option<&'a str> {
    presentation.invalidate();
    if render_notice.is_some() {
        return None;
    }
    let notice = format!("Venus renderer failure: {error}");
    set_title(&notice);
    report(&notice);
    Some(render_notice.insert(notice).as_str())
}

fn can_follow_implicit_resize(message: &ClientMessage) -> bool {
    matches!(message, ClientMessage::Mouse(_))
}

fn shortcut_is_ready(state: ElementState, repeat: bool) -> bool {
    state == ElementState::Pressed && !repeat
}

fn copy_is_ready(selecting: bool, state: ElementState, repeat: bool) -> bool {
    !selecting && shortcut_is_ready(state, repeat)
}

fn selection_presentation_is_ready(message: &ClientMessage, current: bool) -> bool {
    current || matches!(message, ClientMessage::Selection(SelectionAction::Copy))
}

fn dismisses_clipboard_notice(source: LocalNoticeSource) -> bool {
    source == LocalNoticeSource::Input
}

fn terminal_screen(
    workspace: Option<&WorkspaceScene>,
    fallback: PhysicalSize<u32>,
) -> PhysicalSize<u32> {
    workspace.map_or(fallback, |workspace| {
        PhysicalSize::new(
            workspace.terminal.width.max(0.0).round() as u32,
            workspace.terminal.height.max(0.0).round() as u32,
        )
    })
}

fn move_terminal_pointer(
    input: &mut InputState,
    position: PhysicalPosition<f64>,
    workspace: Option<&WorkspaceScene>,
) -> Option<ClientMessage> {
    let (x, y) = workspace.map_or((position.x, position.y), |workspace| {
        (
            position.x - f64::from(workspace.terminal.left),
            position.y - f64::from(workspace.terminal.top),
        )
    });
    input.move_pointer(x, y)
}

fn button_reaches_terminal(
    workspace: bool,
    terminal_hit: bool,
    state: ElementState,
    presentation_current: bool,
) -> bool {
    state == ElementState::Released || presentation_current && (!workspace || terminal_hit)
}

fn ime_allowed(
    window_focused: bool,
    workspace_focus: WorkspaceFocus,
    orbit_attached: bool,
) -> bool {
    orbit_attached && terminal_focused(window_focused, workspace_focus)
}

fn ime_reaches_terminal(allowed: bool, event: &Ime) -> bool {
    allowed || matches!(event, Ime::Disabled)
}

fn native_ime_message(
    input: &mut InputState,
    model: &mut SessionModel,
    event: Ime,
) -> Option<ClientMessage> {
    match input.ime(event) {
        Ok(message) => message,
        Err(detail) => {
            model.set_venus_notice(
                LocalNoticeSource::Input,
                format!("Venus could not encode input: {detail}"),
            );
            None
        }
    }
}

fn terminal_focused(window_focused: bool, workspace_focus: WorkspaceFocus) -> bool {
    window_focused && workspace_focus == WorkspaceFocus::Terminal
}

fn effective_workspace_focus(
    directory_picker: bool,
    workspace_focus: WorkspaceFocus,
) -> WorkspaceFocus {
    if directory_picker {
        WorkspaceFocus::Terminal
    } else {
        workspace_focus
    }
}

fn accessibility_workspace_focus(
    target: AccessibilityTarget,
    queue: impl FnOnce(WorkspaceAction) -> bool,
) -> Option<WorkspaceFocus> {
    let (focus, id) = match target {
        AccessibilityTarget::Terminal => return Some(WorkspaceFocus::Terminal),
        AccessibilityTarget::Tab(id) => (WorkspaceFocus::Tabs, id),
        AccessibilityTarget::Pane(id) => (WorkspaceFocus::Panes, id),
    };
    queue(WorkspaceAction::FocusId(id)).then_some(focus)
}

fn workspace_shortcut(
    code: KeyCode,
    modifiers: orbit_protocol::session::Modifiers,
    active_tab: &str,
) -> Option<WorkspaceAction> {
    use orbit_protocol::session::Modifiers;

    match (modifiers, code) {
        (Modifiers::ALT, KeyCode::KeyH) => Some(WorkspaceAction::Focus(WorkspaceDirection::Left)),
        (Modifiers::ALT, KeyCode::KeyL) => Some(WorkspaceAction::Focus(WorkspaceDirection::Right)),
        (Modifiers::ALT, KeyCode::KeyK) => Some(WorkspaceAction::Focus(WorkspaceDirection::Up)),
        (Modifiers::ALT, KeyCode::KeyJ) => Some(WorkspaceAction::Focus(WorkspaceDirection::Down)),
        (Modifiers::ALT, KeyCode::KeyM) => Some(WorkspaceAction::CreatePane),
        (Modifiers::ALT, KeyCode::KeyZ) => Some(WorkspaceAction::PickTabDirectory),
        (modifiers, KeyCode::KeyT) if modifiers == Modifiers::ALT.union(Modifiers::SHIFT) => {
            Some(WorkspaceAction::CreateTab)
        }
        (modifiers, KeyCode::KeyH) if modifiers == Modifiers::CTRL.union(Modifiers::ALT) => {
            Some(WorkspaceAction::Move(WorkspaceDirection::Left))
        }
        (modifiers, KeyCode::KeyL) if modifiers == Modifiers::CTRL.union(Modifiers::ALT) => {
            Some(WorkspaceAction::Move(WorkspaceDirection::Right))
        }
        (modifiers, KeyCode::KeyK) if modifiers == Modifiers::CTRL.union(Modifiers::ALT) => {
            Some(WorkspaceAction::Move(WorkspaceDirection::Up))
        }
        (modifiers, KeyCode::KeyJ) if modifiers == Modifiers::CTRL.union(Modifiers::ALT) => {
            Some(WorkspaceAction::Move(WorkspaceDirection::Down))
        }
        (modifiers, KeyCode::KeyW) if modifiers == Modifiers::ALT.union(Modifiers::SHIFT) => {
            Some(WorkspaceAction::CloseTab {
                tab: active_tab.into(),
            })
        }
        _ => None,
    }
}

fn sends_workspace_shortcut(
    action: &WorkspaceAction,
    directory_picker_visible: bool,
    state: ElementState,
    repeat: bool,
) -> bool {
    workspace_shortcut_is_sendable(action, directory_picker_visible)
        && state == ElementState::Pressed
        && (!repeat || matches!(action, WorkspaceAction::Focus(_)))
}

fn workspace_shortcut_is_sendable(action: &WorkspaceAction, picker_visible: bool) -> bool {
    !picker_visible
        || matches!(
            action,
            WorkspaceAction::Focus(WorkspaceDirection::Left | WorkspaceDirection::Right)
                | WorkspaceAction::CloseTab { .. }
        )
}

fn clipboard_notice<E: std::fmt::Display>(
    clipboard: wl_clipboard_rs::copy::ClipboardType,
    result: std::result::Result<(), E>,
) -> String {
    use wl_clipboard_rs::copy::ClipboardType;

    result.map_or_else(
        |error| format!("Venus could not write the native clipboard: {error}"),
        |()| match clipboard {
            ClipboardType::Regular => "Text copied to the native clipboard.".into(),
            ClipboardType::Primary => "Text copied to the native primary selection.".into(),
            ClipboardType::Both => {
                "Text copied to the native clipboard and primary selection.".into()
            }
        },
    )
}

fn take_deferred_selection(
    pending: &mut VecDeque<ClientMessage>,
    frame_revision: u64,
) -> Vec<ClientMessage> {
    let mut batch = Vec::new();
    while let Some(mut message) = pending.pop_front() {
        if let ClientMessage::Selection(SelectionAction::Begin {
            frame_revision: revision,
            ..
        }) = &mut message
        {
            if !batch.is_empty() {
                pending.push_front(message);
                break;
            }
            *revision = frame_revision;
        }
        let finished = matches!(
            message,
            ClientMessage::Selection(SelectionAction::Finish { .. } | SelectionAction::Copy)
        );
        batch.push(message);
        if finished {
            break;
        }
    }
    batch
}

fn queue_deferred_selection(pending: &mut VecDeque<ClientMessage>, message: ClientMessage) -> bool {
    if pending.len() >= MAX_DEFERRED_SELECTION_MESSAGES {
        return false;
    }
    pending.push_back(message);
    true
}

fn pointer_sequence_needs_cancel(
    gate: &SelectionGate,
    selecting: bool,
    pending: &VecDeque<ClientMessage>,
) -> bool {
    if matches!(gate, SelectionGate::AwaitingFinish) {
        return true;
    }
    for message in pending {
        match message {
            ClientMessage::Selection(SelectionAction::Begin { .. }) => return false,
            ClientMessage::Selection(
                SelectionAction::Update { .. } | SelectionAction::Finish { .. },
            ) => return true,
            _ => {}
        }
    }
    selecting
}

fn clipboard_paste_message(input: impl Read) -> std::result::Result<ClientMessage, String> {
    let mut text = Vec::new();
    input
        .take(session::MAX_PASTE_BYTES as u64 + 1)
        .read_to_end(&mut text)
        .map_err(|error| error.to_string())?;
    if text.is_empty() {
        return Err("The native clipboard contains no text".into());
    }
    if text.len() > session::MAX_PASTE_BYTES {
        return Err(format!(
            "Native clipboard text exceeds the {} byte paste limit",
            session::MAX_PASTE_BYTES
        ));
    }
    std::str::from_utf8(&text).map_err(|_| "stream did not contain valid UTF-8".to_string())?;
    Ok(ClientMessage::Paste(text))
}

fn clipboard_paste_notice(error: impl std::fmt::Display) -> String {
    format!("Venus could not paste from the native clipboard: {error}")
}

fn write_native_clipboard(
    clipboard: wl_clipboard_rs::copy::ClipboardType,
    text: String,
) -> std::result::Result<(), wl_clipboard_rs::copy::Error> {
    use wl_clipboard_rs::copy::{MimeType, Options, Source};

    let mut options = Options::new();
    options.clipboard(clipboard);
    options.copy(Source::Bytes(text.into_bytes().into()), MimeType::Text)
}

fn read_native_clipboard() -> std::result::Result<impl Read, String> {
    use wl_clipboard_rs::paste::{self, ClipboardType, MimeType, Seat};

    let (pipe, _) = paste::get_contents(ClipboardType::Regular, Seat::Unspecified, MimeType::Text)
        .map_err(|error| error.to_string())?;
    Ok(pipe)
}

fn wayland_clipboard_type(effect: &ClipboardEffect) -> wl_clipboard_rs::copy::ClipboardType {
    use wl_clipboard_rs::copy::ClipboardType;

    match effect {
        ClipboardEffect::SelectionCopy {
            location: ClipboardLocation::Selection,
            ..
        } => ClipboardType::Both,
        ClipboardEffect::SelectionCopy {
            location: ClipboardLocation::Standard,
            ..
        }
        | ClipboardEffect::TerminalWrite {
            location: ClipboardLocation::Standard,
            ..
        } => ClipboardType::Regular,
        ClipboardEffect::SelectionCopy {
            location: ClipboardLocation::Primary,
            ..
        }
        | ClipboardEffect::TerminalWrite {
            location: ClipboardLocation::Selection | ClipboardLocation::Primary,
            ..
        } => ClipboardType::Primary,
    }
}

fn initial_window_size(
    columns: Option<u16>,
    rows: Option<u16>,
    metrics: CellMetrics,
    scale: f64,
    snapshot: Option<&eon_workspace_protocol::v4::Snapshot>,
) -> Result<PhysicalSize<u32>> {
    let (horizontal, vertical) = snapshot.map_or((0.0, 0.0), |snapshot| {
        WorkspaceScene::initial_overhead(snapshot, metrics)
    });
    let size = PhysicalSize::new(
        columns.map_or((960.0 * scale).round() as u32, |columns| {
            (f32::from(columns) * metrics.width + metrics.padding * 2.0 + horizontal).ceil() as u32
        }),
        rows.map_or((600.0 * scale).round() as u32, |rows| {
            (f32::from(rows) * metrics.height + metrics.padding * 2.0 + vertical).ceil() as u32
        }),
    );
    let limit = wgpu::Limits::default().max_texture_dimension_2d;
    // Grid admission depends on header heights, not tab text fitting.
    let workspace = snapshot.map(|snapshot| {
        WorkspaceScene::from_snapshot(snapshot, size, metrics, 0.0, 0.0, |_, _| {
            (String::new(), 0.0)
        })
    });
    let terminal = terminal_screen(workspace.as_ref(), size);
    let admitted = surface_size(terminal, metrics);
    if size.width > limit
        || size.height > limit
        || admitted.is_none_or(|actual| {
            columns.is_some_and(|columns| columns != actual.cols)
                || rows.is_some_and(|rows| rows != actual.rows)
                || ((terminal.width as f32 - metrics.padding * 2.0) / metrics.width).floor()
                    * ((terminal.height as f32 - metrics.padding * 2.0) / metrics.height).floor()
                    > MAX_CELLS as f32
        })
    {
        return Err(
            "initial terminal dimensions exceed the native surface or Orbit grid limits".into(),
        );
    }
    Ok(size)
}

fn surface_size(screen: PhysicalSize<u32>, metrics: CellMetrics) -> Option<SurfaceSize> {
    if screen.width == 0
        || screen.height == 0
        || screen.width > u32::from(u16::MAX)
        || screen.height > u32::from(u16::MAX)
    {
        return None;
    }
    let cell_width = metrics.width.round() as u32;
    let cell_height = metrics.height.round() as u32;
    let padding = metrics.padding.round() as u32;
    let horizontal_padding = padding.checked_mul(2)?;
    let vertical_padding = padding.checked_mul(2)?;
    if horizontal_padding >= screen.width || vertical_padding >= screen.height {
        return None;
    }
    let columns = (screen.width - horizontal_padding) / cell_width;
    let mut rows = (screen.height - vertical_padding) / cell_height;
    if columns == 0 || rows == 0 {
        return None;
    }
    rows = rows.min(MAX_CELLS as u32 / columns);
    Some(SurfaceSize {
        cols: u16::try_from(columns).ok()?,
        rows: u16::try_from(rows).ok()?,
        screen_width: screen.width,
        screen_height: screen.height,
        cell_width,
        cell_height,
        padding_top: padding,
        padding_bottom: screen.height - padding - rows * cell_height,
        padding_left: padding,
        padding_right: screen.width - padding - columns * cell_width,
    })
}

pub(super) fn run(arguments: LaunchArguments) -> Result {
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .map_err(|_| io::Error::other("Venus requires a native Wayland display"))?;
    let mut application = Application::new(arguments, event_loop.create_proxy());
    if application.startup_admission && application.workspace_socket.is_some() {
        let response = yazelix_venus::read_workspace_response(&mut io::stdin().lock())?;
        if !matches!(response, eon_workspace_protocol::v4::Response::Snapshot(_)) {
            return Err("Venus startup admission requires an Eon workspace snapshot".into());
        }
        application.workspace_model.apply(response);
    }
    if application.supervised {
        start_presentation_control(event_loop.create_proxy())?;
    }
    event_loop.run_app(&mut application)?;
    if let Some(error) = application.fatal_error {
        return Err(error.into());
    }
    Ok(())
}

fn start_presentation_control(proxy: EventLoopProxy<UserEvent>) -> Result {
    thread::Builder::new()
        .name("venus-presentation-control".into())
        .spawn(move || {
            run_presentation_control(io::stdin().lock(), |event| proxy.send_event(event).is_ok());
        })?;
    Ok(())
}

fn run_presentation_control(mut input: impl Read, mut send: impl FnMut(UserEvent) -> bool) {
    let mut message = [0; 8];
    while input.read_exact(&mut message).is_ok() {
        if message == *b"present\n" && !send(UserEvent::Present) {
            return;
        }
    }
    let _ = send(UserEvent::Exit);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch;
    use eon_workspace_protocol::v4::{DirectoryPicker, Pane, Snapshot, Tab};

    #[test]
    #[ignore = "requires an isolated native Wayland display"]
    fn typography_without_grid_waits_for_and_validates_workspace_geometry() {
        use winit::platform::wayland::EventLoopBuilderExtWayland;
        struct Probe(Application);
        impl ApplicationHandler<UserEvent> for Probe {
            fn resumed(&mut self, event_loop: &ActiveEventLoop) {
                self.0.resumed(event_loop);
                assert!(self.0.window.is_none(), "wait for workspace geometry");
                self.0
                    .workspace_model
                    .apply(eon_workspace_protocol::v4::Response::Snapshot(Snapshot {
                        active_tab: "t1".into(),
                        directory_picker: None,
                        tabs: vec![Tab {
                            id: "t1".into(),
                            directory: b"/tmp".to_vec(),
                            selected_pane: Some("p1".into()),
                            panes: vec![Pane {
                                id: "p1".into(),
                                session: "s1".into(),
                                endpoint: b"/unused-orbit.sock".to_vec(),
                                live: true,
                            }],
                        }],
                    }));
                self.0.resumed(event_loop);
                assert!(self.0.window.is_none());
                assert!(self.0.transport.is_none());
                assert!(
                    self.0
                        .fatal_error
                        .as_deref()
                        .is_some_and(|error| { error.contains("initial terminal dimensions") })
                );
            }
            fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
        }
        let event_loop = EventLoop::<UserEvent>::with_user_event()
            .with_wayland()
            .with_any_thread(true)
            .build()
            .unwrap();
        let arguments = launch::launch_arguments(
            ["--font-size", "96", "--line-height", "3", "--workspace"]
                .into_iter()
                .map(OsString::from)
                .chain([std::env::temp_dir()
                    .join("venus-unavailable-typography-workspace.sock")
                    .into_os_string()]),
            None,
        )
        .unwrap();
        let mut probe = Probe(Application::new(arguments, event_loop.create_proxy()));
        event_loop.run_app(&mut probe).unwrap();
        assert!(probe.0.fatal_error.is_some());
    }

    #[test]
    #[ignore = "requires an isolated native Wayland display and Vulkan renderer; use fractional output scale"]
    fn native_initial_grid_survives_compositor_scale_admission() {
        native_startup(true);
    }

    #[test]
    #[ignore = "requires an isolated native Wayland display and Vulkan renderer; use fractional output scale"]
    fn native_font_only_startup_completes_without_input() {
        native_startup(false);
    }

    fn native_startup(explicit_grid: bool) {
        use winit::platform::wayland::EventLoopBuilderExtWayland;
        struct Probe {
            app: Application,
            explicit_grid: bool,
            checked: bool,
        }
        impl ApplicationHandler<UserEvent> for Probe {
            fn resumed(&mut self, event_loop: &ActiveEventLoop) {
                self.app.resumed(event_loop);
            }
            fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
                assert!(
                    !matches!(event, UserEvent::Exit),
                    "native startup must complete without a test timer or user input"
                );
                self.app.user_event(event_loop, event);
            }
            fn window_event(
                &mut self,
                event_loop: &ActiveEventLoop,
                id: WindowId,
                event: WindowEvent,
            ) {
                if self.app.initial_scale_pending {
                    assert!(
                        self.app.transport.is_none(),
                        "attach only after native size admission"
                    );
                }
                self.app.window_event(event_loop, id, event);
            }
            fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
                self.app.about_to_wait(event_loop);
                if self.app.initial_scale_pending {
                    return;
                }
                assert!(self.app.transport.is_some());
                let state = self
                    .app
                    .window
                    .as_ref()
                    .expect("the configured native window opens");
                let size = surface_size(state.renderer.size(), state.renderer.metrics()).unwrap();
                if self.explicit_grid {
                    assert_eq!((size.cols, size.rows), (100, 30));
                } else {
                    assert_eq!(
                        state.renderer.size(),
                        LogicalSize::new(960.0, 600.0).to_physical::<u32>(state.scale_factor)
                    );
                }
                self.checked = true;
                event_loop.exit();
            }
        }
        let event_loop = EventLoop::<UserEvent>::with_user_event()
            .with_wayland()
            .with_any_thread(true)
            .build()
            .unwrap();
        let arguments = launch::launch_arguments(
            [
                "--application-id",
                "venus-typography-proof",
                "--no-decorations",
                "--font-size",
                "20",
                "--line-height",
                "1.5",
            ]
            .into_iter()
            .chain(
                explicit_grid
                    .then_some(["--columns", "100", "--rows", "30"])
                    .into_iter()
                    .flatten(),
            )
            .map(OsString::from)
            .chain([std::env::temp_dir()
                .join("venus-unavailable-typography.sock")
                .into_os_string()]),
            None,
        )
        .unwrap();
        let mut probe = Probe {
            app: Application::new(arguments, event_loop.create_proxy()),
            explicit_grid,
            checked: false,
        };
        let proxy = event_loop.create_proxy();
        let (stop, stopped) = std::sync::mpsc::channel::<()>();
        let watchdog = thread::spawn(move || {
            if stopped.recv_timeout(Duration::from_secs(5))
                == Err(std::sync::mpsc::RecvTimeoutError::Timeout)
            {
                let _ = proxy.send_event(UserEvent::Exit);
            }
        });
        event_loop.run_app(&mut probe).unwrap();
        let _ = stop.send(());
        watchdog.join().unwrap();
        assert!(probe.checked);
    }

    #[test]
    fn initial_grid_includes_workspace_chrome_at_each_scale() {
        let mut snapshot = Snapshot {
            active_tab: "t1".into(),
            directory_picker: None,
            tabs: vec![Tab {
                id: "t1".into(),
                directory: b"/tmp".to_vec(),
                selected_pane: Some("p1".into()),
                panes: (1..=3)
                    .map(|i| Pane {
                        id: format!("p{i}"),
                        session: format!("s{i}"),
                        endpoint: format!("/tmp/{i}.sock").into_bytes(),
                        live: true,
                    })
                    .collect(),
            }],
        };
        for picker in [false, true] {
            if picker {
                snapshot.directory_picker = Some(DirectoryPicker {
                    tab: "t1".into(),
                    endpoint: b"/tmp/picker.sock".to_vec(),
                });
            }
            for scale in [1.0, 1.25, 1.5, 2.0] {
                let metrics = CellMetrics::for_scale(scale);
                for snapshot in [None, Some(&snapshot)] {
                    let size =
                        initial_window_size(Some(100), Some(30), metrics, scale, snapshot).unwrap();
                    let workspace = snapshot.map(|s| {
                        WorkspaceScene::from_snapshot(s, size, metrics, 0.0, 0.0, |_, _| {
                            (String::new(), 0.0)
                        })
                    });
                    let actual =
                        surface_size(terminal_screen(workspace.as_ref(), size), metrics).unwrap();
                    assert_eq!((actual.cols, actual.rows), (100, 30));
                    let size =
                        initial_window_size(None, Some(30), metrics, scale, snapshot).unwrap();
                    assert_eq!(size.width, (960.0 * scale).round() as u32);
                }
            }
        }
        let metrics = CellMetrics::for_scale(1.0);
        assert!(initial_window_size(Some(u16::MAX), Some(30), metrics, 1.0, None).is_err());
        assert!(initial_window_size(Some(500), Some(300), metrics, 1.0, None).is_err());
    }

    #[test]
    #[ignore = "requires an isolated native Wayland display and Vulkan renderer"]
    fn live_output_keeps_application_input_admitted_before_repaint() {
        use eon_workspace_protocol::v4 as workspace;
        use orbit_protocol::{
            Capabilities, Colors, Cursor, CursorShape, Dimensions, Frame, Rgb, Screen,
        };
        use std::{
            os::unix::net::{UnixListener, UnixStream},
            sync::mpsc,
        };
        use winit::{event::DeviceId, platform::wayland::EventLoopBuilderExtWayland};

        fn frame(revision: u64, screen: Screen) -> TransportEvent {
            TransportEvent::Server(ServerMessage::Frame(Box::new(Frame {
                revision,
                dimensions: Dimensions { cols: 0, rows: 0 },
                screen,
                scroll_position: orbit_protocol::ScrollPosition::default(),
                title: format!("output {revision}"),
                working_directory: String::new(),
                capabilities: Capabilities {
                    hyperlinks: true,
                    kitty_graphics: false,
                },
                colors: Colors {
                    background: Rgb::BLACK,
                    foreground: Rgb::BLACK,
                    cursor: None,
                    palette: [Rgb::BLACK; orbit_protocol::PALETTE_LEN],
                },
                cursor: Cursor {
                    visible: false,
                    blinking: false,
                    password_input: false,
                    shape: CursorShape::Block,
                    viewport: None,
                },
                rows: Vec::new(),
            })))
        }

        fn linked_frame(revision: u64) -> TransportEvent {
            use orbit_protocol::{Cell, CellStyle, CellWidth, Row, StyleColor, Underline};
            let TransportEvent::Server(ServerMessage::Frame(mut frame)) =
                frame(revision, Screen::Alternate)
            else {
                unreachable!()
            };
            frame.dimensions = Dimensions { cols: 2, rows: 1 };
            let cell = Cell {
                width: CellWidth::Wide,
                text: "界".into(),
                hyperlink: format!("https://example.com/{revision}"),
                style: CellStyle {
                    foreground: StyleColor::None,
                    background: StyleColor::None,
                    underline_color: StyleColor::None,
                    underline: Underline::None,
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
                },
            };
            frame.rows = vec![Row {
                wrapped: false,
                wrap_continuation: false,
                kitty_virtual_placeholder: false,
                cells: vec![
                    cell.clone(),
                    Cell {
                        width: CellWidth::SpacerTail,
                        text: String::new(),
                        ..cell
                    },
                ],
            }];
            TransportEvent::Server(ServerMessage::Frame(frame))
        }

        struct Probe {
            app: Application,
            listener: UnixListener,
            stream: Option<UnixStream>,
            actions: mpsc::Receiver<WorkspaceAction>,
            checked: bool,
        }
        impl ApplicationHandler<UserEvent> for Probe {
            fn resumed(&mut self, event_loop: &ActiveEventLoop) {
                self.app.create_window(event_loop).unwrap();
                self.stream = Some(self.listener.accept().unwrap().0);
                self.app
                    .handle_transport(TransportEvent::Server(ServerMessage::Attached));
                assert!(
                    self.app.status().is_empty(),
                    "normal workspace attachment must not announce its first-frame wait"
                );
                self.app.handle_transport(frame(1, Screen::Primary));
            }

            fn window_event(
                &mut self,
                event_loop: &ActiveEventLoop,
                id: WindowId,
                event: WindowEvent,
            ) {
                self.app.window_event(event_loop, id, event);
                if self.checked || self.app.presented_revision() != Some(1) {
                    return;
                }
                let app = &mut self.app;
                let device_id = DeviceId::dummy();
                let scene = app.workspace_scene().unwrap();
                let terminal = PhysicalPosition::new(
                    f64::from(scene.terminal.left + 30.0),
                    f64::from(scene.terminal.top + 30.0),
                );
                let tab = scene.tabs[1].rect;
                app.window_event(
                    event_loop,
                    id,
                    WindowEvent::CursorMoved {
                        device_id,
                        position: terminal,
                    },
                );
                let generation = app.presentation.generation;
                app.handle_transport(frame(2, Screen::Primary));
                // Exercise the actual handlers in the output-to-presentation gap.
                app.window_event(
                    event_loop,
                    id,
                    WindowEvent::MouseWheel {
                        device_id,
                        delta: MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, 8.0)),
                        phase: TouchPhase::Started,
                    },
                );
                assert!(
                    app.terminal_scroll.phase_active,
                    "first wheel event was dropped after output"
                );
                app.handle_transport(frame(3, Screen::Primary));
                app.window_event(
                    event_loop,
                    id,
                    WindowEvent::MouseWheel {
                        device_id,
                        delta: MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, 0.0)),
                        phase: TouchPhase::Ended,
                    },
                );
                assert!(
                    !app.terminal_scroll.phase_active,
                    "gesture End was dropped after output"
                );
                assert!(app.send_workspace(WorkspaceAction::Focus(WorkspaceDirection::Right)));
                assert_eq!(
                    self.actions.recv_timeout(Duration::from_secs(2)).unwrap(),
                    WorkspaceAction::Focus(WorkspaceDirection::Right)
                );
                app.window_event(
                    event_loop,
                    id,
                    WindowEvent::CursorMoved {
                        device_id,
                        position: PhysicalPosition::new(
                            f64::from(tab.left + 2.0),
                            f64::from(tab.top + 2.0),
                        ),
                    },
                );
                app.window_event(
                    event_loop,
                    id,
                    WindowEvent::MouseInput {
                        device_id,
                        state: ElementState::Pressed,
                        button: MouseButton::Left,
                    },
                );
                assert_eq!(
                    self.actions.recv_timeout(Duration::from_secs(2)).unwrap(),
                    WorkspaceAction::FocusId("t2".into())
                );
                app.window_event(
                    event_loop,
                    id,
                    WindowEvent::CursorMoved {
                        device_id,
                        position: terminal,
                    },
                );
                app.window_event(
                    event_loop,
                    id,
                    WindowEvent::MouseInput {
                        device_id,
                        state: ElementState::Pressed,
                        button: MouseButton::Left,
                    },
                );
                app.handle_transport(frame(4, Screen::Primary));
                assert!(
                    app.input.is_selecting(),
                    "selection did not start: {:?}, resize {:?}, terminal {:?}",
                    app.model.notice(),
                    app.last_resize,
                    app.terminal_size()
                );
                app.window_event(
                    event_loop,
                    id,
                    WindowEvent::CursorMoved {
                        device_id,
                        position: PhysicalPosition::new(terminal.x + 20.0, terminal.y),
                    },
                );
                app.window_event(
                    event_loop,
                    id,
                    WindowEvent::MouseInput {
                        device_id,
                        state: ElementState::Released,
                        button: MouseButton::Left,
                    },
                );
                assert!(
                    app.deferred_selection.is_empty(),
                    "live output deferred the gesture"
                );
                assert_eq!(app.selection_gate, SelectionGate::AwaitingFinish);
                assert!(
                    app.presentation.generation > generation,
                    "renderer cache must refresh"
                );
                assert_eq!(
                    app.presented_revision(),
                    Some(1),
                    "unpainted content is not presented"
                );
                app.handle_transport(TransportEvent::Server(ServerMessage::SelectionFinished {
                    frame_revision: 4,
                }));
                assert_eq!(app.selection_gate, SelectionGate::AwaitingPresentation(4));
                app.render();
                assert_eq!(app.presented_revision(), Some(4));
                assert_eq!(app.selection_gate, SelectionGate::Ready);

                // A workspace poll can skip intermediate pane selections.
                // Extra headers must resize the retained attachment too.
                let previous_size = app.last_resize.unwrap();
                let mut snapshot = app.workspace_model.snapshot().unwrap().clone();
                app.active_endpoint = Some(snapshot.tabs[0].panes[0].endpoint.clone());
                app.active_endpoint_live = true;
                snapshot.tabs[0].panes.push(Pane {
                    id: "p3".into(),
                    session: "s3".into(),
                    endpoint: b"/unused/p3.sock".to_vec(),
                    live: false,
                });
                app.handle_workspace(WorkspaceEvent::Response(workspace::Response::Snapshot(
                    snapshot,
                )));
                let resized = surface_size(
                    app.terminal_size().unwrap(),
                    app.window.as_ref().unwrap().renderer.metrics(),
                )
                .unwrap();
                assert!(resized.rows < previous_size.rows);

                let stream = self.stream.as_mut().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                let mut messages = Vec::new();
                loop {
                    let mut header = [0; session::HEADER_BYTES];
                    match stream.read_exact(&mut header) {
                        Ok(()) => {}
                        Err(error)
                            if matches!(
                                error.kind(),
                                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                            ) =>
                        {
                            break;
                        }
                        Err(error) => panic!("input transport failed: {error}"),
                    }
                    let mut bytes = Vec::from(header);
                    bytes.resize(session::client_message_len(&header).unwrap().unwrap(), 0);
                    stream
                        .read_exact(&mut bytes[session::HEADER_BYTES..])
                        .unwrap();
                    messages.push(session::decode_client_message(&bytes).unwrap());
                }
                assert!(
                    messages
                        .iter()
                        .any(|message| matches!(message, ClientMessage::PreviewVertical { .. }))
                );
                assert!(
                    messages.contains(&ClientMessage::Resize(resized)),
                    "changed pane geometry did not resize the retained attachment"
                );
                let selections: Vec<_> = messages
                    .into_iter()
                    .filter_map(|message| match message {
                        ClientMessage::Selection(action) => Some(action),
                        _ => None,
                    })
                    .collect();
                assert!(
                    matches!(
                        selections.as_slice(),
                        [
                            SelectionAction::Begin {
                                frame_revision: 1,
                                ..
                            },
                            SelectionAction::Update { .. },
                            SelectionAction::Finish { .. },
                        ]
                    ),
                    "unexpected selection wire sequence: {selections:?}"
                );

                // Geometry and attachment transitions still require a real presentation.
                app.handle_transport(frame(5, Screen::Alternate));
                assert_eq!(app.presented_revision(), None);
                assert!(!app.send_workspace(WorkspaceAction::Focus(WorkspaceDirection::Right)));
                app.render();
                assert_eq!(app.presented_revision(), Some(5));
                app.window_event(
                    event_loop,
                    id,
                    WindowEvent::Resized(PhysicalSize::new(900, 600)),
                );
                assert_eq!(app.presented_revision(), None);
                app.render();
                app.window_focused = true;
                app.links.pointer_inside = true;
                app.handle_transport(linked_frame(6));
                let workspace = app.workspace_scene().unwrap();
                let metrics = app.window.as_ref().unwrap().renderer.metrics();
                app.cursor = PhysicalPosition::new(
                    f64::from(workspace.terminal.left + metrics.padding + metrics.width * 1.5),
                    f64::from(workspace.terminal.top + metrics.padding + metrics.height * 0.5),
                );
                assert!(app.pointer_link().is_none(), "unpresented target is inert");
                app.render();
                assert_eq!(app.focused_link().unwrap().uri, "https://example.com/6");
                app.render();
                app.links.pressed = app.links.focus;
                app.handle_transport(linked_frame(7));
                assert!(app.focused_link().is_none());
                app.render();
                assert!(app.handle_link_button(ElementState::Released, MouseButton::Left));
                assert!(
                    app.link_opener.is_none(),
                    "a captured click cannot cross a frame replacement"
                );
                app.render();
                assert_eq!(app.focused_link().unwrap().uri, "https://example.com/7");
                let unknown = PhysicalKey::Unidentified(winit::keyboard::NativeKeyCode::Xkb(240));
                app.input.set_modifiers(
                    winit::keyboard::ModifiersState::CONTROL
                        | winit::keyboard::ModifiersState::SHIFT,
                );
                assert!(app.handle_link_key(
                    PhysicalKey::Code(KeyCode::KeyO),
                    ElementState::Pressed,
                    false
                ));
                assert!(app.links.keyboard);
                assert!(
                    app.handle_link_key(unknown, ElementState::Pressed, false),
                    "inspection must capture unidentified native keys"
                );
                assert!(app.handle_link_key(
                    PhysicalKey::Code(KeyCode::Escape),
                    ElementState::Pressed,
                    false
                ));
                assert!(!app.links.keyboard);
                assert!(
                    app.handle_link_key(unknown, ElementState::Pressed, true),
                    "captured repeats remain captured after dismissal"
                );
                assert!(app.handle_link_key(unknown, ElementState::Released, false));
                assert!(
                    !app.handle_link_key(unknown, ElementState::Pressed, false),
                    "fresh keys return to terminal routing"
                );

                app.window_event(
                    event_loop,
                    id,
                    WindowEvent::Resized(PhysicalSize::new(100, 600)),
                );
                let scene = app.workspace_scene().unwrap();
                assert!(scene.tab_scroll_limit() > 0.0);
                for position in [
                    PhysicalPosition::new(10.0, 1.0),
                    PhysicalPosition::new(
                        f64::from(scene.tabs[0].rect.right() + 1.0),
                        f64::from(scene.tab_viewport.height / 2.0),
                    ),
                ] {
                    app.tab_scroll = 0.0;
                    app.presentation.invalidate();
                    app.render();
                    app.cursor = position;
                    assert_eq!(
                        app.workspace_scene()
                            .unwrap()
                            .hit_test(position.x as f32, position.y as f32),
                        None
                    );
                    app.window_event(
                        event_loop,
                        id,
                        WindowEvent::MouseWheel {
                            device_id,
                            delta: MouseScrollDelta::LineDelta(0.0, -1.0),
                            phase: TouchPhase::Moved,
                        },
                    );
                    assert!(
                        app.tab_scroll > 0.0,
                        "wheel over tab-strip gaps must scroll tabs"
                    );
                }
                app.set_orbit_attachment(b"replacement".to_vec(), false);
                assert!(app.status().contains("selected Eon pane is offline"));
                assert_eq!(app.presented_revision(), None);
                assert!(app.pointer_link().is_none());
                self.checked = true;
                event_loop.exit();
            }
        }

        let root = std::env::temp_dir().join(format!("venus-live-input-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let orbit_socket = root.join("orbit.sock");
        let workspace_socket = root.join("workspace.sock");
        let listener = UnixListener::bind(&orbit_socket).unwrap();
        let workspace_listener = UnixListener::bind(&workspace_socket).unwrap();
        let snapshot = Snapshot {
            active_tab: "t1".into(),
            directory_picker: None,
            tabs: (1..=2)
                .map(|index| Tab {
                    id: format!("t{index}"),
                    directory: b"/tmp".to_vec(),
                    selected_pane: Some(format!("p{index}")),
                    panes: vec![Pane {
                        id: format!("p{index}"),
                        session: format!("s{index}"),
                        endpoint: root
                            .join(format!("pane-{index}.sock"))
                            .into_os_string()
                            .into_vec(),
                        live: true,
                    }],
                })
                .collect(),
        };
        let response = workspace::Response::Snapshot(snapshot.clone());
        let (sender, actions) = mpsc::channel();
        let server = thread::spawn(move || {
            for _ in 0..2 {
                loop {
                    let (mut stream, _) = workspace_listener.accept().unwrap();
                    let mut header = [0; workspace::HEADER_BYTES];
                    stream.read_exact(&mut header).unwrap();
                    let mut bytes = Vec::from(header);
                    bytes.resize(workspace::declared_message_len(&header).unwrap(), 0);
                    stream
                        .read_exact(&mut bytes[workspace::HEADER_BYTES..])
                        .unwrap();
                    let action = workspace::decode_request(&bytes).unwrap().action;
                    stream
                        .write_all(&workspace::encode_response(&response).unwrap())
                        .unwrap();
                    if action != WorkspaceAction::Inspect {
                        sender.send(action).unwrap();
                        break;
                    }
                }
            }
        });
        let event_loop = EventLoop::<UserEvent>::with_user_event()
            .with_wayland()
            .with_any_thread(true)
            .build()
            .unwrap();
        let mut app = Application::new(
            launch::launch_arguments(
                [
                    "--font-family",
                    "DejaVu Sans Mono",
                    "--font-size",
                    "20",
                    "--line-height",
                    "1.5",
                ]
                .into_iter()
                .map(OsString::from)
                .chain([orbit_socket.into_os_string()]),
                None,
            )
            .unwrap(),
            event_loop.create_proxy(),
        );
        // This input fixture supplies its own attachment and frames; native
        // startup admission has separate coverage.
        app.initial_scale_pending = false;
        assert!(app.status().starts_with("Connecting to Orbit"));
        app.workspace_model
            .apply(workspace::Response::Snapshot(snapshot));
        assert!(
            app.status().is_empty(),
            "normal workspace attachment must not announce its connection"
        );
        app.workspace_transport = Some(WorkspaceTransport::start(workspace_socket, || {}));
        let mut probe = Probe {
            app,
            listener,
            stream: None,
            actions,
            checked: false,
        };
        event_loop.run_app(&mut probe).unwrap();
        assert!(probe.checked);
        drop(probe);
        server.join().unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn terminal_scroll_keeps_fractional_input_and_elapsed_time_physics() {
        let start = Instant::now();
        let mut scroll = TerminalScroll::default();

        scroll.push_lines(0.4, 20.0);
        scroll.push_lines(0.6, 20.0);
        assert_eq!(scroll.pixels, 60.0);

        scroll.cancel();
        scroll.push_pixels(0.0, TouchPhase::Started, start, 20.0);
        scroll.push_pixels(
            60.0,
            TouchPhase::Moved,
            start + Duration::from_millis(100),
            20.0,
        );
        scroll.push_pixels(
            0.0,
            TouchPhase::Ended,
            start + Duration::from_millis(100),
            20.0,
        );
        assert_eq!(scroll.pixels, 120.0);
        assert_eq!(scroll.velocity, 1_200.0);

        let before = scroll.pixels;
        scroll.advance(start + Duration::from_millis(200));
        let one_tick = scroll.pixels - before;

        let mut split = TerminalScroll::default();
        split.push_pixels(0.0, TouchPhase::Started, start, 20.0);
        split.push_pixels(
            60.0,
            TouchPhase::Moved,
            start + Duration::from_millis(100),
            20.0,
        );
        split.push_pixels(
            0.0,
            TouchPhase::Ended,
            start + Duration::from_millis(100),
            20.0,
        );
        let before = split.pixels;
        split.advance(start + Duration::from_millis(150));
        split.advance(start + Duration::from_millis(200));
        assert!(((split.pixels - before) - one_tick).abs() < 1e-9);

        let mut incomplete = TerminalScroll::default();
        incomplete.push_pixels(100.0, TouchPhase::Moved, start, 20.0);
        assert_eq!(incomplete.velocity, 0.0);

        let mut after_wheel = TerminalScroll::default();
        after_wheel.push_lines(1.0, 20.0);
        after_wheel.push_pixels(0.0, TouchPhase::Moved, start, 20.0);
        after_wheel.push_pixels(
            60.0,
            TouchPhase::Moved,
            start + Duration::from_millis(50),
            20.0,
        );
        after_wheel.push_pixels(
            0.0,
            TouchPhase::Ended,
            start + Duration::from_millis(50),
            20.0,
        );
        assert_eq!(after_wheel.velocity, 2_400.0);

        let mut capped = TerminalScroll::default();
        capped.push_pixels(0.0, TouchPhase::Started, start, 20.0);
        capped.push_pixels(
            1_000.0,
            TouchPhase::Moved,
            start + Duration::from_millis(1),
            20.0,
        );
        capped.push_pixels(
            0.0,
            TouchPhase::Ended,
            start + Duration::from_millis(1),
            20.0,
        );
        assert_eq!(capped.velocity, MAX_FLING_VELOCITY);

        let mut bounded = TerminalScroll::default();
        for _ in 0..=MAX_SCROLL_SAMPLES {
            bounded.push_pixels(1.0, TouchPhase::Moved, start, 20.0);
        }
        assert_eq!(bounded.samples.len(), MAX_SCROLL_SAMPLES);

        let mut expired = TerminalScroll::default();
        expired.push_pixels(10.0, TouchPhase::Started, start, 20.0);
        expired.push_pixels(
            10.0,
            TouchPhase::Moved,
            start + SCROLL_SAMPLE_WINDOW + Duration::from_millis(1),
            20.0,
        );
        expired.push_pixels(
            0.0,
            TouchPhase::Ended,
            start + SCROLL_SAMPLE_WINDOW + Duration::from_millis(1),
            20.0,
        );
        assert_eq!(expired.velocity, 0.0);

        let mut paused = TerminalScroll::default();
        paused.push_pixels(0.0, TouchPhase::Started, start, 20.0);
        paused.push_pixels(
            60.0,
            TouchPhase::Moved,
            start + Duration::from_millis(50),
            20.0,
        );
        paused.push_pixels(
            0.0,
            TouchPhase::Ended,
            start + Duration::from_millis(250),
            20.0,
        );
        assert_eq!(paused.velocity, 0.0);

        let mut interrupted = TerminalScroll::default();
        interrupted.push_pixels(0.0, TouchPhase::Started, start, 20.0);
        interrupted.push_pixels(
            60.0,
            TouchPhase::Moved,
            start + Duration::from_millis(50),
            20.0,
        );
        interrupted.push_lines(1.0, 20.0);
        interrupted.push_pixels(
            0.0,
            TouchPhase::Ended,
            start + Duration::from_millis(50),
            20.0,
        );
        assert_eq!(interrupted.velocity, 0.0);

        let mut cancelled = TerminalScroll::default();
        cancelled.push_pixels(20.0, TouchPhase::Cancelled, start, 20.0);
        assert_eq!(cancelled.pixels, 0.0);
        assert_eq!(cancelled.terminal_lines, 0.0);
    }

    #[test]
    fn terminal_scroll_coalesces_one_signed_batch_without_losing_distance() {
        let start = Instant::now();
        assert!(!TerminalScroll::default().accept_batch(-1, -1, 20.0));
        let row = yazelix_venus::DrawRow {
            wrapped: false,
            wrap_continuation: false,
            kitty_virtual_placeholder: false,
            cells: Vec::new(),
        };
        let preview = ScenePreview::Viewport {
            frame_revision: 7,
            direction: VerticalDirection::Up,
            edge_reached: false,
            rows: vec![row.clone(); 5],
        };
        let mut continuous = TerminalScroll::default();
        continuous.push_pixels(20.0, TouchPhase::Moved, start, 20.0);
        assert_eq!(
            continuous.next_request(8, Some(&preview), 20.0),
            Some(ClientMessage::ScrollVertical {
                frame_revision: 7,
                rows: -2,
            })
        );

        let mut scroll = TerminalScroll::default();
        scroll.push_pixels(50.0, TouchPhase::Moved, start, 20.0);

        assert_eq!(
            scroll.next_request(7, None, 20.0),
            Some(ClientMessage::PreviewVertical {
                frame_revision: 7,
                direction: VerticalDirection::Up,
            })
        );
        assert_eq!(scroll.next_request(7, None, 20.0), None);
        scroll.preview_arrived(7, VerticalDirection::Up);
        scroll.resolve_preview(Some(&preview));
        assert_eq!(scroll.offset(Some(&preview), 20.0), 100.0);
        assert_eq!(
            scroll.next_request(7, Some(&preview), 20.0),
            Some(ClientMessage::ScrollVertical {
                frame_revision: 7,
                rows: -5,
            })
        );

        scroll.push_pixels(40.0, TouchPhase::Moved, start, 20.0);
        scroll.rebase();
        assert_eq!(scroll.next_request(8, None, 20.0), None);
        assert!(!scroll.accept_batch(-4, -4, 20.0));
        assert!(scroll.accept_batch(-5, -5, 20.0));
        assert_eq!(scroll.pixels, 80.0);

        let mut cancelled = TerminalScroll::default();
        cancelled.push_pixels(20.0, TouchPhase::Moved, start, 20.0);
        cancelled.next_request(7, None, 20.0);
        cancelled.preview_arrived(7, VerticalDirection::Up);
        cancelled.resolve_preview(Some(&preview));
        cancelled.next_request(7, Some(&preview), 20.0);
        cancelled.cancel();
        assert!(!cancelled.active());
        cancelled.push_pixels(20.0, TouchPhase::Moved, start, 20.0);
        assert_eq!(cancelled.next_request(7, Some(&preview), 20.0), None);
        assert!(cancelled.accept_batch(-2, -2, 20.0));
        assert_eq!(cancelled.pixels, 40.0);

        let down_preview = ScenePreview::Viewport {
            frame_revision: 7,
            direction: VerticalDirection::Down,
            edge_reached: true,
            rows: vec![row; 3],
        };
        let mut down = TerminalScroll::default();
        down.push_pixels(-50.0, TouchPhase::Moved, start, 20.0);
        down.preview_arrived(7, VerticalDirection::Down);
        down.resolve_preview(Some(&down_preview));
        assert_eq!(down.offset(Some(&down_preview), 20.0), -60.0);
        assert_eq!(
            down.next_request(7, Some(&down_preview), 20.0),
            Some(ClientMessage::ScrollVertical {
                frame_revision: 7,
                rows: 3,
            })
        );

        assert_eq!(
            scroll.next_request(9, None, 20.0),
            Some(ClientMessage::PreviewVertical {
                frame_revision: 9,
                direction: VerticalDirection::Up,
            })
        );
        scroll.rebase();
        assert_eq!(
            scroll.next_request(10, None, 20.0),
            Some(ClientMessage::PreviewVertical {
                frame_revision: 10,
                direction: VerticalDirection::Up,
            })
        );

        let edge = ScenePreview::Viewport {
            frame_revision: 10,
            direction: VerticalDirection::Up,
            edge_reached: true,
            rows: Vec::new(),
        };
        scroll.preview_arrived(10, VerticalDirection::Up);
        scroll.resolve_preview(Some(&edge));
        assert_eq!(scroll.pixels, 0.0);
        assert_eq!(scroll.velocity, 0.0);

        let mut terminal = TerminalScroll::default();
        terminal.push_lines(1.0, 20.0);
        let routed = ScenePreview::TerminalOwned {
            frame_revision: 7,
            direction: VerticalDirection::Up,
        };
        assert_eq!(terminal.resolve_preview(Some(&routed)), Some(1.0));
        assert_eq!(terminal.pixels, 0.0);

        terminal.push_pixels(20.0, TouchPhase::Moved, start, 20.0);
        assert_eq!(terminal.resolve_preview(Some(&routed)), Some(1.0));
        assert_eq!(terminal.pixels, 0.0);

        terminal.push_pixels(0.0, TouchPhase::Started, start, 20.0);
        terminal.push_pixels(
            20.0,
            TouchPhase::Moved,
            start + Duration::from_millis(50),
            20.0,
        );
        assert_eq!(terminal.resolve_preview(Some(&routed)), Some(1.0));
        terminal.push_pixels(
            0.0,
            TouchPhase::Ended,
            start + Duration::from_millis(50),
            20.0,
        );
        assert_eq!(terminal.velocity, 0.0);
    }

    #[test]
    fn metadata_endpoints_are_only_live_panes_in_the_active_tab() {
        let snapshot = Snapshot {
            active_tab: "t1".into(),
            tabs: vec![
                Tab {
                    id: "t1".into(),
                    directory: b"/tmp/eon".to_vec(),
                    selected_pane: Some("pane-1".into()),
                    panes: vec![
                        Pane {
                            id: "pane-1".into(),
                            session: "session-1".into(),
                            endpoint: b"one".to_vec(),
                            live: true,
                        },
                        Pane {
                            id: "pane-2".into(),
                            session: "session-2".into(),
                            endpoint: b"offline".to_vec(),
                            live: false,
                        },
                    ],
                },
                Tab {
                    id: "t2".into(),
                    directory: b"/tmp/nova".to_vec(),
                    selected_pane: Some("pane-3".into()),
                    panes: vec![Pane {
                        id: "pane-3".into(),
                        session: "session-3".into(),
                        endpoint: b"hidden".to_vec(),
                        live: true,
                    }],
                },
            ],
            directory_picker: None,
        };

        assert_eq!(
            visible_metadata_endpoints(&snapshot, Some(b"one"), false),
            HashSet::new()
        );
        assert_eq!(
            visible_metadata_endpoints(&snapshot, Some(b"one"), true),
            [b"one".to_vec()].into()
        );

        let picker = Snapshot {
            directory_picker: Some(DirectoryPicker {
                tab: "t1".into(),
                endpoint: b"picker".to_vec(),
            }),
            ..snapshot
        };
        assert_eq!(
            visible_metadata_endpoints(&picker, Some(b"picker"), true),
            HashSet::new()
        );

        let inactive_picker = Snapshot {
            active_tab: "t2".into(),
            ..picker
        };
        assert_eq!(
            visible_metadata_endpoints(&inactive_picker, Some(b"hidden"), true),
            [b"hidden".to_vec()].into()
        );
    }

    #[test]
    fn renderer_failure_uses_non_gpu_diagnostics_until_success() {
        let mut notice = None;
        let mut presentation = PresentationState::default();
        let candidate = presentation.candidate(Some(7));
        presentation.publish(candidate);
        let mut titles = Vec::new();
        let mut diagnostics = Vec::new();
        let mut alerts = 0;

        for error in ["the Venus glyph atlas is full", "a later renderer failure"] {
            if record_render_failure(
                &mut notice,
                &mut presentation,
                error,
                |title| titles.push(title.to_owned()),
                |message| diagnostics.push(format!("venus: {message}")),
            )
            .is_some()
            {
                alerts += 1;
            }
        }

        let failure = "Venus renderer failure: the Venus glyph atlas is full";
        assert!(!presentation.has_presented_geometry());
        assert_eq!(titles, [failure]);
        assert_eq!(diagnostics, [format!("venus: {failure}")]);
        assert_eq!(alerts, 1);
        assert_eq!(
            window_title(Some("Orbit title"), notice.as_deref()),
            failure
        );

        assert!(notice.take().is_some());
        assert_eq!(
            window_title(Some("Orbit title"), notice.as_deref()),
            "Orbit title"
        );
        assert_eq!(window_title(Some(""), None), "Venus");
    }

    #[test]
    fn launch_values_project_to_native_window_attributes() {
        let parse = |arguments: &[&str]| {
            launch::launch_arguments(arguments.iter().map(OsString::from), None).unwrap()
        };

        let default = parse(&[]);
        let attributes = window_attributes(
            default.decorations,
            default.background_opacity,
            default.background_blur,
        );
        assert!(attributes.decorations && !attributes.transparent && !attributes.blur);

        for value in ["0", "0.88", "1"] {
            let parsed = parse(&[
                "--no-decorations",
                "--background-opacity",
                value,
                "--background-blur",
                "--workspace",
                "eon.sock",
            ]);
            let attributes = window_attributes(
                parsed.decorations,
                parsed.background_opacity,
                parsed.background_blur,
            );
            assert!(attributes.blur);
            assert_eq!(attributes.transparent, value != "1");
        }
    }

    #[test]
    fn cursor_animation_deadline_stops_when_inactive_unfocused_or_occluded() {
        let now = Instant::now();
        let mut deadline = None;

        assert!(!update_animation_deadline(true, true, &mut deadline, now));
        assert_eq!(deadline, Some(now + ANIMATION_FRAME_INTERVAL));
        assert!(update_animation_deadline(
            true,
            true,
            &mut deadline,
            now + ANIMATION_FRAME_INTERVAL,
        ));
        assert_eq!(
            deadline,
            Some(now + ANIMATION_FRAME_INTERVAL.saturating_mul(2))
        );

        for (active, focused, occluded) in [
            (false, true, false),
            (true, false, false),
            (true, true, true),
        ] {
            let allowed = cursor_animation_allowed(focused, occluded);
            assert!(!update_animation_deadline(
                active,
                allowed,
                &mut deadline,
                now,
            ));
            assert_eq!(deadline, None);
        }
    }

    #[test]
    fn surface_measurements_match_orbit_invariants() {
        let metrics = CellMetrics::for_scale(1.0);
        let size = surface_size(PhysicalSize::new(960, 600), metrics).unwrap();
        assert_eq!((size.cols, size.rows), (93, 32));
        assert!(
            orbit_protocol::session::encode_client_message(&ClientMessage::Resize(size)).is_ok()
        );
        for screen in [PhysicalSize::new(929, 976), PhysicalSize::new(4096, 4096)] {
            let size = surface_size(screen, metrics).unwrap();
            assert_eq!(
                size.padding_left + u32::from(size.cols) * size.cell_width + size.padding_right,
                screen.width
            );
            assert_eq!(
                size.padding_top + u32::from(size.rows) * size.cell_height + size.padding_bottom,
                screen.height
            );
        }
        assert!(surface_size(PhysicalSize::new(1, 1), metrics).is_none());
        assert!(surface_size(PhysicalSize::new(25, 600), metrics).is_none());
        assert!(surface_size(PhysicalSize::new(960, 25), metrics).is_none());
        assert!(surface_size(PhysicalSize::new(u32::from(u16::MAX) + 1, 600), metrics).is_none());
    }

    #[test]
    fn presentation_control_emits_complete_commands_then_exit() {
        let mut events = Vec::new();

        run_presentation_control(&b"ignored\npresent\npart"[..], |event| {
            events.push(match event {
                UserEvent::Present => "present",
                UserEvent::Exit => "exit",
                _ => panic!("unexpected presentation control event"),
            });
            true
        });

        assert_eq!(events, ["present", "exit"]);
    }

    #[test]
    fn blinking_starts_with_a_complete_visible_phase_after_idle() {
        let now = Instant::now();
        let mut visible = false;
        let mut deadline = Some(now);

        assert!(!update_blink(false, &mut visible, &mut deadline, now));
        assert_eq!((visible, deadline), (true, None));
        assert!(!update_blink(true, &mut visible, &mut deadline, now));
        assert_eq!((visible, deadline), (true, Some(now + BLINK_INTERVAL)));
    }

    #[test]
    fn orbit_retry_schedule_is_bounded_and_resettable() {
        let now = Instant::now();
        let mut retry = OrbitRetry::default();

        for delay in [250, 500, 1_000, 2_000, 4_000, 5_000, 5_000] {
            retry.schedule(now);
            assert_eq!(retry.deadline, Some(now + Duration::from_millis(delay)));
            retry.schedule(now + Duration::from_millis(1));
            assert_eq!(retry.deadline, Some(now + Duration::from_millis(delay)));
            assert!(!retry.take_due(now + Duration::from_millis(delay - 1)));
            assert!(retry.take_due(now + Duration::from_millis(delay)));
        }

        retry.schedule(now);
        retry.reset();
        assert_eq!(retry.deadline, None);
        retry.schedule(now);
        assert_eq!(retry.deadline, Some(now + Duration::from_millis(250)));
    }

    #[test]
    fn workspace_recovery_requires_the_same_selected_live_endpoint() {
        let first = b"/run/eon/session-1.sock";
        let second = b"/run/eon/session-2.sock";

        assert!(retry_is_allowed(false, None, None));
        assert!(retry_is_allowed(true, Some((first, true)), Some(first)));
        assert!(!retry_is_allowed(true, Some((first, false)), Some(first)));
        assert!(!retry_is_allowed(true, Some((second, true)), Some(first)));
    }

    #[test]
    fn only_supervised_orbit_busy_is_retryable() {
        let busy = TransportEvent::Server(ServerMessage::Busy);

        assert!(managed_busy_is_retryable(true, &busy));
        assert!(!managed_busy_is_retryable(false, &busy));
        assert!(!managed_busy_is_retryable(
            true,
            &TransportEvent::Server(ServerMessage::Accepted),
        ));
    }

    #[test]
    fn protocol_and_terminal_failures_do_not_become_socket_retries() {
        use orbit_protocol::session::Failure;

        for (code, suppressed) in [
            (FailureCode::InvalidInput, false),
            (FailureCode::Protocol, true),
            (FailureCode::Terminal, true),
        ] {
            assert_eq!(
                server_failure_suppresses_retry(&ServerMessage::Failure(Failure {
                    code,
                    detail: "bounded".into(),
                })),
                suppressed
            );
        }
    }

    #[test]
    fn retryable_loss_records_state_independently_of_retry_policy() {
        use orbit_protocol::session::Failure;

        let attached = || {
            let mut model = SessionModel::new();
            model.apply(ServerMessage::Attached).unwrap();
            model
        };
        let fail = |model: &mut SessionModel, code, detail: &str| {
            model
                .apply(ServerMessage::Failure(Failure {
                    code,
                    detail: detail.into(),
                }))
                .unwrap();
        };
        let mut suppressed = attached();
        fail(&mut suppressed, FailureCode::Terminal, "request rejected");
        assert_eq!(
            suppressed.notice(),
            Some("Orbit terminal failure: request rejected")
        );
        suppressed.apply(ServerMessage::Accepted).unwrap();
        assert_eq!(suppressed.notice(), None);
        fail(
            &mut suppressed,
            FailureCode::InvalidInput,
            "later rejected input",
        );
        assert_eq!(
            suppressed.notice(),
            Some("Orbit rejected input: later rejected input")
        );

        assert!(!apply_retryable_loss(
            &mut suppressed,
            "Orbit closed the local session".into(),
            true,
        ));
        assert!(matches!(
            suppressed.connection(),
            ConnectionState::Lost { detail } if detail == "Orbit closed the local session"
        ));
        assert!(!suppressed.is_attached());
        assert_eq!(suppressed.notice(), None);

        let mut protocol_failure = attached();
        fail(
            &mut protocol_failure,
            FailureCode::Protocol,
            "invalid mouse coordinates",
        );
        protocol_failure.set_venus_notice(LocalNoticeSource::Input, "late local notice");
        assert!(!apply_retryable_loss(
            &mut protocol_failure,
            "Orbit closed the local session".into(),
            true,
        ));
        assert_eq!(
            protocol_failure.notice(),
            Some("Orbit protocol failure: invalid mouse coordinates")
        );
        assert!(matches!(
            protocol_failure.connection(),
            ConnectionState::Lost { .. }
        ));

        let mut eligible = attached();
        assert!(apply_retryable_loss(
            &mut eligible,
            "Cannot read from Orbit: connection reset".into(),
            false,
        ));
        assert!(matches!(
            eligible.connection(),
            ConnectionState::Lost { detail }
                if detail == "Cannot read from Orbit: connection reset"
        ));
    }

    #[test]
    fn stalled_link_dispatcher_is_bounded_and_reaped() {
        let child = Command::new("sleep").arg("30").spawn().unwrap();
        let pid = child.id();
        let now = Instant::now();
        let mut opener = LinkOpener {
            child,
            deadline: now + Duration::from_secs(10),
        };
        assert!(opener.poll(now).is_none());
        assert!(
            opener
                .poll(now + Duration::from_secs(10))
                .unwrap()
                .contains("timed out")
        );
        drop(opener);
        assert!(
            !PathBuf::from(format!("/proc/{pid}")).exists(),
            "dispatcher must be reaped"
        );
    }

    #[test]
    fn hyperlink_actions_require_current_presentation_and_safe_exact_uri() {
        let mut presentation = PresentationState::default();
        let first = presentation.candidate(Some(7));
        assert!(!presentation.is_current(first));
        presentation.publish(first);
        assert!(presentation.is_current(first));
        presentation.content_changed();
        assert!(!presentation.is_current(first));
        let second = presentation.candidate(Some(8));
        assert!(!presentation.is_current(second));
        presentation.publish(second);
        assert!(presentation.is_current(second));
        presentation.unpublish();
        assert!(!presentation.is_current(second));
        presentation.invalidate();
        presentation.publish(first);
        assert!(!presentation.is_current(first));

        for uri in [
            "https://example.com/exact?x=%26&y=2#part",
            "HTTP://localhost:8080/",
            "https://[::1]:443/a",
        ] {
            assert_eq!(validate_open_uri(uri), Ok(()), "{uri}");
        }
        for uri in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "https:///bad",
            "https://user@example.com/",
            "https://example.com\\@evil.test",
            "https://example.com/%zz",
            "https://example.com/%0",
            "https://example.com/\n",
            "https://example.com/\u{202e}",
            "https://[bad]/",
            "https://example.com:99999/",
            "https://-bad.example/",
            "https://example.com/ space",
        ] {
            assert!(validate_open_uri(uri).is_err(), "{uri:?}");
        }
        assert!(validate_open_uri(&format!("https://example.com/{}", "x".repeat(4096))).is_err());
        assert!(validate_copy_uri("file:///tmp/a").is_ok());
        assert!(validate_copy_uri("https://example.com/\n").is_err());
        let target = "https://example.com/\u{202e}abc";
        let escaped = escaped_link(target);
        assert!(escaped.is_ascii());
        assert!(escaped.contains("\\u{202e}"));
        let pages = (0..escaped.len().div_ceil(7))
            .map(|page| link_page(&escaped, page, 7))
            .collect::<String>();
        assert_eq!(pages, escaped);
    }

    #[test]
    fn attachment_revision_collision_waits_for_the_new_generation() {
        let mut presentation = PresentationState::default();
        let attachment_a = presentation.candidate(Some(1));
        presentation.publish(attachment_a);
        assert_eq!(presentation.presented_revision(), Some(1));

        presentation.invalidate();
        let attachment_b = presentation.candidate(Some(1));
        assert_ne!(attachment_b, attachment_a);
        assert_eq!(presentation.presented_revision(), None);

        presentation.publish(attachment_b);
        assert_eq!(presentation.presented_revision(), Some(1));
    }

    #[test]
    fn output_keeps_presented_geometry_until_a_structural_transition() {
        let mut presentation = PresentationState::default();
        let workspace_a = presentation.candidate(Some(7));
        presentation.publish(workspace_a);
        assert!(presentation.has_presented_geometry());

        presentation.content_changed();
        let output = presentation.candidate(Some(8));
        assert!(output.generation > workspace_a.generation);
        assert!(presentation.has_presented_geometry());
        assert_eq!(presentation.presented_revision(), Some(7));
        presentation.publish(workspace_a);
        assert_eq!(presentation.presented_revision(), Some(7));
        presentation.publish(output);
        assert_eq!(presentation.presented_revision(), Some(8));

        presentation.invalidate();
        let workspace_b = presentation.candidate(Some(7));
        assert!(!presentation.has_presented_geometry());
        presentation.publish(workspace_a);
        assert!(!presentation.has_presented_geometry());

        presentation.publish(workspace_b);
        assert!(presentation.has_presented_geometry());
        presentation.unpublish();
        assert!(!presentation.has_presented_geometry());

        presentation.publish(workspace_b);
        assert!(presentation.has_presented_geometry());
    }

    #[test]
    fn published_accessibility_focus_waits_for_eon_queue_admission() {
        for (target, admitted, expected_focus, expected_action) in [
            (
                AccessibilityTarget::Tab("t2".into()),
                true,
                Some(WorkspaceFocus::Tabs),
                WorkspaceAction::FocusId("t2".into()),
            ),
            (
                AccessibilityTarget::Pane("pane-3".into()),
                false,
                None,
                WorkspaceAction::FocusId("pane-3".into()),
            ),
        ] {
            let mut queued = None;
            let focus = accessibility_workspace_focus(target, |action| {
                queued = Some(action);
                admitted
            });
            assert_eq!((focus, queued), (expected_focus, Some(expected_action)));
        }
        assert_eq!(
            accessibility_workspace_focus(AccessibilityTarget::Terminal, |_| unreachable!()),
            Some(WorkspaceFocus::Terminal)
        );
    }

    #[test]
    fn workspace_chrome_preserves_terminal_release_pairing() {
        use ElementState::{Pressed, Released};

        for (workspace, terminal_hit, state, current, expected) in [
            (true, false, Pressed, true, false),
            (true, false, Released, false, true),
            (true, true, Pressed, true, true),
            (false, false, Pressed, true, true),
            (false, false, Pressed, false, false),
        ] {
            assert_eq!(
                button_reaches_terminal(workspace, terminal_hit, state, current),
                expected
            );
        }
    }

    #[test]
    fn direct_workspace_shortcuts_use_stable_eon_actions() {
        use orbit_protocol::session::Modifiers;

        for (key, modifiers, action) in [
            (
                KeyCode::KeyH,
                Modifiers::ALT,
                WorkspaceAction::Focus(WorkspaceDirection::Left),
            ),
            (
                KeyCode::KeyL,
                Modifiers::ALT,
                WorkspaceAction::Focus(WorkspaceDirection::Right),
            ),
            (
                KeyCode::KeyK,
                Modifiers::ALT,
                WorkspaceAction::Focus(WorkspaceDirection::Up),
            ),
            (
                KeyCode::KeyJ,
                Modifiers::ALT,
                WorkspaceAction::Focus(WorkspaceDirection::Down),
            ),
            (KeyCode::KeyM, Modifiers::ALT, WorkspaceAction::CreatePane),
            (
                KeyCode::KeyZ,
                Modifiers::ALT,
                WorkspaceAction::PickTabDirectory,
            ),
            (
                KeyCode::KeyT,
                Modifiers::ALT.union(Modifiers::SHIFT),
                WorkspaceAction::CreateTab,
            ),
            (
                KeyCode::KeyH,
                Modifiers::CTRL.union(Modifiers::ALT),
                WorkspaceAction::Move(WorkspaceDirection::Left),
            ),
            (
                KeyCode::KeyL,
                Modifiers::CTRL.union(Modifiers::ALT),
                WorkspaceAction::Move(WorkspaceDirection::Right),
            ),
            (
                KeyCode::KeyK,
                Modifiers::CTRL.union(Modifiers::ALT),
                WorkspaceAction::Move(WorkspaceDirection::Up),
            ),
            (
                KeyCode::KeyJ,
                Modifiers::CTRL.union(Modifiers::ALT),
                WorkspaceAction::Move(WorkspaceDirection::Down),
            ),
            (
                KeyCode::KeyW,
                Modifiers::ALT.union(Modifiers::SHIFT),
                WorkspaceAction::CloseTab { tab: "t2".into() },
            ),
        ] {
            assert_eq!(workspace_shortcut(key, modifiers, "t2"), Some(action));
        }
        for key in [KeyCode::KeyT, KeyCode::KeyW] {
            for modifiers in [
                Modifiers::CTRL,
                Modifiers::ALT,
                Modifiers::CTRL.union(Modifiers::SHIFT),
                Modifiers::CTRL
                    .union(Modifiers::ALT)
                    .union(Modifiers::SHIFT),
            ] {
                assert_eq!(workspace_shortcut(key, modifiers, "t2"), None);
            }
        }
        assert_eq!(
            workspace_shortcut(KeyCode::KeyH, Modifiers::ALT.union(Modifiers::SHIFT), "t2"),
            None
        );
    }

    #[test]
    fn creation_shortcuts_ignore_repeat_while_traversal_may_repeat() {
        assert!(!sends_workspace_shortcut(
            &WorkspaceAction::CreateTab,
            false,
            ElementState::Pressed,
            true
        ));
        assert!(sends_workspace_shortcut(
            &WorkspaceAction::Focus(WorkspaceDirection::Right),
            false,
            ElementState::Pressed,
            true
        ));
        for direction in [WorkspaceDirection::Left, WorkspaceDirection::Right] {
            assert!(sends_workspace_shortcut(
                &WorkspaceAction::Focus(direction),
                true,
                ElementState::Pressed,
                false
            ));
        }
        assert!(!sends_workspace_shortcut(
            &WorkspaceAction::Focus(WorkspaceDirection::Up),
            true,
            ElementState::Pressed,
            false
        ));
        assert!(!sends_workspace_shortcut(
            &WorkspaceAction::CreatePane,
            false,
            ElementState::Released,
            false
        ));
        assert!(!sends_workspace_shortcut(
            &WorkspaceAction::PickTabDirectory,
            true,
            ElementState::Pressed,
            false
        ));
        for action in [
            WorkspaceAction::Move(WorkspaceDirection::Left),
            WorkspaceAction::CloseTab { tab: "t2".into() },
        ] {
            assert!(!sends_workspace_shortcut(
                &action,
                false,
                ElementState::Pressed,
                true
            ));
        }
        assert!(!sends_workspace_shortcut(
            &WorkspaceAction::Move(WorkspaceDirection::Left),
            true,
            ElementState::Pressed,
            false
        ));
        assert!(sends_workspace_shortcut(
            &WorkspaceAction::CloseTab { tab: "t2".into() },
            true,
            ElementState::Pressed,
            false
        ));
    }

    #[test]
    fn ime_requires_current_attached_terminal_but_always_cleans_up() {
        assert!(!ime_allowed(false, WorkspaceFocus::Terminal, true));
        assert!(!ime_allowed(true, WorkspaceFocus::Panes, true));
        assert!(!ime_allowed(true, WorkspaceFocus::Terminal, false));
        assert!(ime_allowed(true, WorkspaceFocus::Terminal, true));
        assert!(ime_reaches_terminal(false, &Ime::Disabled));
        assert!(!ime_reaches_terminal(false, &Ime::Commit("ignored".into())));
        assert!(ime_reaches_terminal(true, &Ime::Commit("accepted".into())));
    }

    #[test]
    fn rejected_native_ime_commit_uses_the_input_notice_owner() {
        let mut input = InputState::default();
        let mut model = SessionModel::new();
        let rejected = "x".repeat(session::MAX_KEY_TEXT_BYTES + 1);

        assert_eq!(
            native_ime_message(&mut input, &mut model, Ime::Commit(rejected)),
            None
        );
        assert_eq!(
            model.notice(),
            Some(
                "Venus could not encode input: native input method commit is not accepted semantic key text"
            )
        );

        assert!(matches!(
            native_ime_message(&mut input, &mut model, Ime::Commit("界".into())),
            Some(ClientMessage::Key(_))
        ));
        assert!(model.clear_venus_notice(LocalNoticeSource::Input));
        assert_eq!(model.notice(), None);
    }

    #[test]
    fn orbit_focus_requires_window_and_terminal_focus() {
        assert!(terminal_focused(true, WorkspaceFocus::Terminal));
        assert!(!terminal_focused(false, WorkspaceFocus::Terminal));
        assert!(!terminal_focused(true, WorkspaceFocus::Tabs));
        assert!(!terminal_focused(true, WorkspaceFocus::Panes));
        assert_eq!(
            effective_workspace_focus(true, WorkspaceFocus::Panes),
            WorkspaceFocus::Terminal
        );
        assert_eq!(
            effective_workspace_focus(false, WorkspaceFocus::Panes),
            WorkspaceFocus::Panes
        );
    }

    #[test]
    fn revision_bound_selection_waits_for_a_recovered_surface() {
        let mouse = ClientMessage::Mouse(orbit_protocol::session::MouseEvent {
            action: orbit_protocol::session::MouseAction::Motion,
            button: None,
            modifiers: orbit_protocol::session::Modifiers::empty(),
            x: 0.0,
            y: 0.0,
        });
        let selection = ClientMessage::Selection(SelectionAction::Begin {
            frame_revision: 7,
            position: orbit_protocol::session::SelectionPosition { x: 0.0, y: 0.0 },
            time_ns: 0,
            modifiers: orbit_protocol::session::Modifiers::empty(),
        });

        assert!(can_follow_implicit_resize(&mouse));
        assert!(!can_follow_implicit_resize(&selection));
    }

    #[test]
    fn copy_is_revision_free_but_waits_for_selection_finish() {
        use winit::event::ElementState::{Pressed, Released};

        let copy = ClientMessage::Selection(SelectionAction::Copy);
        assert!(selection_presentation_is_ready(&copy, false));
        assert!(!copy_is_ready(true, Pressed, false));
        assert!(copy_is_ready(false, Pressed, false));
        assert!(!copy_is_ready(false, Pressed, true));
        assert!(!copy_is_ready(false, Released, false));
    }

    #[test]
    fn resize_does_not_dismiss_clipboard_result() {
        assert!(!dismisses_clipboard_notice(LocalNoticeSource::Resize));
        assert!(dismisses_clipboard_notice(LocalNoticeSource::Input));
    }

    #[test]
    fn clipboard_results_are_attributed_without_copying_terminal_cells() {
        use wl_clipboard_rs::copy::ClipboardType::{Both, Regular};

        assert_eq!(
            clipboard_notice(Regular, Ok::<(), &str>(())),
            "Text copied to the native clipboard."
        );
        assert_eq!(
            clipboard_notice(Both, Ok::<(), &str>(())),
            "Text copied to the native clipboard and primary selection."
        );
        assert_eq!(
            clipboard_notice(Regular, Err("display unavailable")),
            "Venus could not write the native clipboard: display unavailable"
        );
    }

    #[test]
    fn deferred_selection_preserves_order_bounds_and_cancel_ownership() {
        let begin = |revision| {
            ClientMessage::Selection(SelectionAction::Begin {
                frame_revision: revision,
                position: orbit_protocol::session::SelectionPosition { x: 1.0, y: 2.0 },
                time_ns: 3,
                modifiers: orbit_protocol::session::Modifiers::empty(),
            })
        };
        let finish = ClientMessage::Selection(SelectionAction::Finish {
            position: orbit_protocol::session::SelectionPosition { x: 1.0, y: 2.0 },
            modifiers: orbit_protocol::session::Modifiers::empty(),
        });
        let mut pending = VecDeque::from([
            begin(1),
            finish.clone(),
            begin(1),
            finish.clone(),
            ClientMessage::Selection(SelectionAction::Copy),
        ]);

        let first = take_deferred_selection(&mut pending, 7);
        assert!(matches!(
            first.as_slice(),
            [
                ClientMessage::Selection(SelectionAction::Begin {
                    frame_revision: 7,
                    ..
                }),
                ClientMessage::Selection(SelectionAction::Finish { .. })
            ]
        ));
        assert!(matches!(
            pending.front(),
            Some(ClientMessage::Selection(SelectionAction::Begin { .. }))
        ));

        assert_eq!(take_deferred_selection(&mut pending, 8).len(), 2);
        assert_eq!(
            take_deferred_selection(&mut pending, 9),
            [ClientMessage::Selection(SelectionAction::Copy)]
        );
        assert!(pending.is_empty());

        pending.resize(
            MAX_DEFERRED_SELECTION_MESSAGES,
            ClientMessage::Selection(SelectionAction::Copy),
        );
        assert!(!queue_deferred_selection(
            &mut pending,
            ClientMessage::Selection(SelectionAction::Copy)
        ));

        let local_begin =
            VecDeque::from([ClientMessage::Selection(SelectionAction::Copy), begin(1)]);
        let server_finish = VecDeque::from([finish]);
        let ready = SelectionGate::Ready;
        let empty = VecDeque::new();
        assert!(!pointer_sequence_needs_cancel(&ready, true, &local_begin));
        assert!(pointer_sequence_needs_cancel(&ready, false, &server_finish));
        assert!(pointer_sequence_needs_cancel(
            &SelectionGate::AwaitingFinish,
            false,
            &empty
        ));
        assert!(pointer_sequence_needs_cancel(&ready, true, &empty));
        assert!(!pointer_sequence_needs_cancel(&ready, false, &empty));
    }

    #[test]
    fn selection_gate_waits_for_finish_and_its_authoritative_presentation() {
        let mut gate = SelectionGate::Ready;

        assert!(gate.admits(7));
        gate.finish_sent();
        assert!(!gate.admits(7));

        gate.finish_received(8);
        assert!(!gate.admits(7));
        assert!(gate.admits(8));

        gate.finish_sent();
        gate.finish_received(8);
        assert!(gate.admits(9));
    }

    #[test]
    fn native_clipboard_text_becomes_one_bounded_semantic_paste() {
        let text = "first\n界\0second";
        assert_eq!(
            clipboard_paste_message(text.as_bytes()),
            Ok(ClientMessage::Paste(text.as_bytes().to_vec()))
        );
        assert_eq!(
            clipboard_paste_message(&b""[..]).unwrap_err(),
            "The native clipboard contains no text"
        );
        let mut oversized = io::Cursor::new(vec![b'x'; session::MAX_PASTE_BYTES + 2]);
        assert_eq!(
            clipboard_paste_message(&mut oversized).unwrap_err(),
            format!(
                "Native clipboard text exceeds the {} byte paste limit",
                session::MAX_PASTE_BYTES
            )
        );
        assert_eq!(oversized.position(), session::MAX_PASTE_BYTES as u64 + 1);
        assert_eq!(
            clipboard_paste_message(&[0xff][..]).unwrap_err(),
            "stream did not contain valid UTF-8"
        );
        assert_eq!(
            clipboard_paste_notice("display unavailable"),
            "Venus could not paste from the native clipboard: display unavailable"
        );
    }

    #[test]
    fn clipboard_effects_use_distinct_wayland_targets() {
        use wl_clipboard_rs::copy::ClipboardType::{Both, Primary, Regular};

        assert!(matches!(
            wayland_clipboard_type(&ClipboardEffect::SelectionCopy {
                location: ClipboardLocation::Selection,
                text: String::new(),
            }),
            Both
        ));
        assert!(matches!(
            wayland_clipboard_type(&ClipboardEffect::SelectionCopy {
                location: ClipboardLocation::Standard,
                text: String::new(),
            }),
            Regular
        ));
        assert!(matches!(
            wayland_clipboard_type(&ClipboardEffect::TerminalWrite {
                location: ClipboardLocation::Selection,
                text: String::new(),
            }),
            Primary
        ));
    }
}
