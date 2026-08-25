use crate::{Result, launch::LaunchArguments};
use accesskit::Action as AccessibilityAction;
use accesskit_winit::{Event as AccessKitEvent, WindowEvent as AccessKitWindowEvent};
use eon_workspace_protocol::{Action as WorkspaceAction, Direction as WorkspaceDirection};
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
    window::{UserAttentionType, Window, WindowAttributes, WindowId},
};
use yazelix_venus::{
    Accessibility, AccessibilityTarget, CellMetrics, Color, ConnectionState, InputState,
    LocalNoticeSource, MetadataEvent, MetadataTransport, ModelError, PaneMetadata, PresentOutcome,
    Renderer, ScenePreview, SessionModel, Transport, TransportEvent, WorkspaceEvent,
    WorkspaceFocus, WorkspaceHit, WorkspaceModel, WorkspaceScene, WorkspaceTransport,
};

const BLINK_INTERVAL: Duration = Duration::from_millis(500);
const ANIMATION_FRAME_INTERVAL: Duration = Duration::from_millis(16);
const INITIAL_RETRY_DELAY: Duration = Duration::from_millis(250);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(5);
const SCROLL_SAMPLE_WINDOW: Duration = Duration::from_millis(150);
const MAX_SCROLL_SAMPLES: usize = 256;
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
        let preview_matches = match preview {
            Some(ScenePreview::TerminalOwned {
                frame_revision: revision,
                direction: candidate,
            }) => *revision == frame_revision && *candidate == direction,
            Some(ScenePreview::Viewport {
                frame_revision: revision,
                direction: candidate,
                ..
            }) => *revision == frame_revision && *candidate == direction,
            None => false,
        };
        if !preview_matches {
            if self.preview_pending != Some((frame_revision, direction)) {
                self.preview_pending = Some((frame_revision, direction));
                return Some(ClientMessage::PreviewVertical {
                    frame_revision,
                    direction,
                });
            }
            return None;
        }
        let ScenePreview::Viewport { rows, .. } = preview? else {
            return None;
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
            frame_revision,
            rows: requested,
        })
    }

    fn accept_batch(&mut self, requested_rows: i16, applied_rows: i16, cell_height: f64) -> bool {
        let Some(expected_rows) = self.in_flight else {
            return true;
        };
        if expected_rows != requested_rows {
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

    fn invalidate(&mut self) {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("presentation generation exhausted");
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

    fn is_current(&self, candidate: PresentationIdentity) -> bool {
        self.presented == Some(candidate)
    }

    fn current_revision(&self, candidate: PresentationIdentity) -> Option<u64> {
        self.is_current(candidate)
            .then_some(candidate.revision)
            .flatten()
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
    orbit_socket: PathBuf,
    workspace_socket: Option<PathBuf>,
    supervised: bool,
    decorations: bool,
    background_opacity: f32,
    background_blur: bool,
    cursor_tail: Option<(Color, f32)>,
    proxy: EventLoopProxy<UserEvent>,
    window: Option<WindowState>,
    transport: Option<Transport>,
    workspace_transport: Option<WorkspaceTransport>,
    metadata_observers: HashMap<Vec<u8>, MetadataObserver>,
    model: SessionModel,
    workspace_model: WorkspaceModel,
    input: InputState,
    last_resize: Option<SurfaceSize>,
    presentation: PresentationState,
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
            decorations,
            background_opacity,
            background_blur,
            cursor_tail,
        } = arguments;
        Self {
            application_id,
            orbit_socket,
            workspace_socket,
            supervised,
            decorations,
            background_opacity,
            background_blur,
            cursor_tail,
            proxy,
            window: None,
            transport: None,
            workspace_transport: None,
            metadata_observers: HashMap::new(),
            model: SessionModel::new(),
            workspace_model: WorkspaceModel::default(),
            input: InputState::default(),
            last_resize: None,
            presentation: PresentationState::default(),
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
            tab_scroll: 0.0,
            pane_scroll: 0.0,
            terminal_scroll: TerminalScroll::default(),
            cursor: PhysicalPosition::new(0.0, 0.0),
        }
    }

    fn create_window(&mut self, event_loop: &ActiveEventLoop) -> Result {
        let attributes = window_attributes(
            self.decorations,
            self.background_opacity,
            self.background_blur,
        );
        #[cfg(target_os = "linux")]
        let attributes = attributes.with_name(self.application_id.as_str(), "yazelix-venus");
        let window = Arc::new(event_loop.create_window(attributes)?);
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
        ))?;
        let scale_factor = window.scale_factor();

        self.window = Some(WindowState {
            renderer,
            scale_factor,
            adapter,
            accessibility,
            window,
        });
        if let Some(socket) = self.workspace_socket.clone() {
            let proxy = self.proxy.clone();
            self.workspace_transport = Some(WorkspaceTransport::start(socket, move || {
                let _ = proxy.send_event(UserEvent::Workspace);
            }));
        } else {
            self.start_orbit(self.orbit_socket.clone());
        }
        self.refresh_client_view();
        Ok(())
    }

    fn start_orbit(&mut self, socket: PathBuf) {
        let proxy = self.proxy.clone();
        self.orbit_socket = socket.clone();
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
        let Some(state) = &self.window else {
            return;
        };
        let Some(snapshot) = self.workspace_model.snapshot() else {
            return;
        };
        let scene = WorkspaceScene::from_snapshot(
            snapshot,
            state.renderer.size(),
            state.renderer.metrics(),
            0.0,
            0.0,
        );
        self.tab_scroll = scene.active_tab_scroll();
        self.pane_scroll = scene.selected_pane_scroll();
    }

    fn set_workspace_focus(&mut self, focus: WorkspaceFocus) {
        if self.workspace_focus == focus {
            return;
        }
        let was_focused = terminal_focused(self.window_focused, self.workspace_focus);
        self.cancel_terminal_scroll();
        self.workspace_focus = focus;
        let is_focused = terminal_focused(self.window_focused, self.workspace_focus);
        if was_focused != is_focused {
            let message = self.input.terminal_focus(is_focused);
            self.send(message);
        }
        self.refresh_client_view();
    }

    fn set_orbit_attachment(&mut self, endpoint: Vec<u8>, live: bool) {
        self.terminal_scroll.reset();
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
        self.start_orbit(self.orbit_socket.clone());
        self.refresh_client_view();
    }

    fn handle_workspace(&mut self, event: WorkspaceEvent) {
        let received_snapshot = matches!(
            &event,
            WorkspaceEvent::Response(eon_workspace_protocol::Response::Snapshot(_))
        );
        let unavailable = matches!(&event, WorkspaceEvent::Unavailable(_));
        let (view_changed, snapshot_changed) = match event {
            WorkspaceEvent::Response(response) => self.workspace_model.apply(response),
            WorkspaceEvent::Unavailable(detail) => {
                (self.workspace_model.mark_unavailable(detail), false)
            }
        };
        if unavailable && !self.metadata_observers.is_empty() {
            self.metadata_observers.clear();
            self.presentation.invalidate();
        }
        if snapshot_changed {
            self.cancel_terminal_scroll();
            self.reveal_workspace_selection();
            self.presentation.invalidate();
            if let Some((endpoint, live)) = self.workspace_model.active_attachment()
                && (self.active_endpoint.as_deref() != Some(endpoint)
                    || self.active_endpoint_live != live)
            {
                self.set_orbit_attachment(endpoint.to_vec(), live);
            }
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
        let workspace = self.workspace_scene();
        let candidate = self.presentation_candidate(workspace.as_ref());
        self.presentation.is_current(candidate) && self.queue_workspace(action)
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
        if self.workspace_model.snapshot().is_none() {
            return false;
        }
        let code = match event.physical_key {
            PhysicalKey::Code(code) => code,
            PhysicalKey::Unidentified(_) => {
                return self.workspace_focus != WorkspaceFocus::Terminal;
            }
        };
        if code == KeyCode::F6 {
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
        let shortcut = workspace_shortcut(code, self.input.modifiers());
        let returns_to_terminal =
            self.workspace_focus != WorkspaceFocus::Terminal && code == KeyCode::Escape;
        if self.input.consumes_workspace_shortcut(
            code,
            event.state,
            event.repeat,
            shortcut.is_some() || returns_to_terminal,
        ) {
            if returns_to_terminal && event.state == ElementState::Pressed {
                self.set_workspace_focus(WorkspaceFocus::Terminal);
            } else if let Some(action) = shortcut
                .filter(|action| sends_workspace_shortcut(action, event.state, event.repeat))
            {
                self.send_workspace(action);
            }
            return true;
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
                let scroll_rejected = matches!(&message, ServerMessage::Failure(_));
                let plain_frame = matches!(&message, ServerMessage::Frame(_));
                if server_failure_suppresses_retry(&message) {
                    self.retry_suppressed = true;
                }
                let was_attached = self.model.is_attached();
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
                    Ok(Some((location, text))) => self.write_clipboard(location, text),
                    Ok(None) => {}
                    Err(error) => self.model.mark_lost(error.to_string()),
                }
                if accepted {
                    if scroll_rejected {
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
                if accepted_frame {
                    self.presentation.invalidate();
                }
                if frame
                    && self
                        .model
                        .scene()
                        .is_some_and(|scene| !scene.has_selected_content())
                {
                    self.input.cancel_selection();
                }
                if !was_attached && self.model.is_attached() {
                    self.reset_cursor_animation();
                    self.orbit_retry.reset();
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

    fn refresh_client_view(&mut self) {
        let status = self.status();
        let workspace = self.workspace_scene();
        let ime_allowed = ime_allowed(
            self.window_focused,
            self.workspace_focus,
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
        state.accessibility.update(
            &mut state.adapter,
            self.model.scene(),
            workspace.as_ref(),
            self.workspace_focus,
            &status,
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

    fn write_clipboard(&mut self, location: ClipboardLocation, text: String) {
        let result = write_native_clipboard(location, text);
        self.model
            .set_venus_notice(LocalNoticeSource::Clipboard, clipboard_notice(result));
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
        if let Some(notice) = self.model.notice() {
            return notice.to_owned();
        }
        if let Some(socket) = &self.workspace_socket
            && self.workspace_model.snapshot().is_none()
        {
            return format!("Connecting to Eon workspace at {}", socket.display());
        }
        match self.model.connection() {
            ConnectionState::Connecting => {
                format!("Connecting to Orbit at {}", self.orbit_socket.display())
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
        let candidate = self.presentation_candidate(workspace.as_ref());
        let scroll_offset = self.window.as_ref().map_or(0.0, |state| {
            self.terminal_scroll.offset(
                self.model.scroll_preview(),
                f64::from(state.renderer.metrics().height),
            )
        });
        let kinetic_active = self.terminal_scroll.velocity != 0.0;
        let mut refresh = false;
        let Some(state) = &mut self.window else {
            return;
        };
        match state.renderer.render(
            self.model.scene(),
            self.model.scroll_preview(),
            scroll_offset,
            workspace.as_ref(),
            self.workspace_focus,
            status,
            self.blink_visible,
            preedit,
            candidate.generation,
        ) {
            Ok(PresentOutcome::Presented) => {
                refresh = self.render_notice.take().is_some();
                self.presentation.publish(candidate);
                if kinetic_active {
                    state.window.request_redraw();
                }
            }
            Ok(PresentOutcome::Deferred) => self.terminal_scroll.stop_gesture(),
            Ok(PresentOutcome::Recovered) => {
                self.presentation.unpublish();
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
                        self.workspace_focus,
                        notice,
                        state.renderer.size(),
                        state.renderer.metrics(),
                    );
                }
            }
        }
        if refresh {
            self.refresh_client_view();
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
            && let Err(error) = self.create_window(event_loop)
        {
            report(error);
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
        let candidate = self.presentation_candidate(workspace.as_ref());
        let presentation_current = self.presentation.is_current(candidate);
        let presented_revision = self.presentation.current_revision(candidate);
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
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.terminal_scroll.cancel();
                state.scale_factor = scale_factor;
            }
            WindowEvent::Occluded(occluded) => {
                self.window_occluded = occluded;
                self.next_animation = None;
                self.terminal_scroll.cancel();
                state.renderer.reset_cursor_animation();
                if !occluded {
                    state.window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => self.render(),
            WindowEvent::ModifiersChanged(modifiers) => {
                self.cancel_terminal_scroll();
                self.input.set_modifiers(modifiers.state());
            }
            WindowEvent::KeyboardInput { event, .. } => {
                self.cancel_terminal_scroll();
                if self
                    .input
                    .suppresses_retired_key(event.physical_key, event.state, event.repeat)
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
                    if copy_is_ready(
                        presented_revision.is_some(),
                        self.input.is_selecting(),
                        event.state,
                        event.repeat,
                    ) {
                        self.send(ClientMessage::Selection(SelectionAction::Copy));
                    }
                } else if let Some(message) = self.input.key(&event)
                    && (self.send(message) || event.state == ElementState::Released)
                {
                    self.input.commit_key(event.physical_key, event.state);
                }
            }
            WindowEvent::Ime(event) => {
                self.cancel_terminal_scroll();
                let allowed = ime_allowed(
                    self.window_focused,
                    self.workspace_focus,
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
                let message = self
                    .input
                    .native_focus(focused, terminal_focused(focused, self.workspace_focus));
                self.send(message);
                self.refresh_client_view();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = position;
                if presented_revision.is_none() {
                    return;
                }
                let motion = move_terminal_pointer(&mut self.input, position, workspace.as_ref());
                if self.input.is_selecting() {
                    let screen = terminal_screen(workspace.as_ref(), state.renderer.size());
                    let size = surface_size(screen, state.renderer.metrics());
                    if let Some(message) = size.and_then(|size| self.input.selection_motion(size))
                        && self.send(message.clone())
                    {
                        self.input.commit_selection(&message);
                    }
                } else if workspace.as_ref().is_none_or(|workspace| {
                    matches!(
                        workspace.hit_test(position.x as f32, position.y as f32),
                        Some(WorkspaceHit::Terminal)
                    )
                }) && let Some(message) = motion
                {
                    self.send(message);
                }
            }
            WindowEvent::CursorLeft { .. } => self.cancel_terminal_scroll(),
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
                let hit = workspace.as_ref().and_then(|workspace| {
                    workspace.hit_test(self.cursor.x as f32, self.cursor.y as f32)
                });
                if presentation_current
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
                    presentation_current,
                ) {
                    return;
                }
                let _ = move_terminal_pointer(&mut self.input, self.cursor, workspace.as_ref());
                let screen = terminal_screen(workspace.as_ref(), renderer_size);
                let size = surface_size(screen, metrics);
                let selection = size.and_then(|size| {
                    self.input
                        .selection_button(button_state, button, size, presented_revision)
                });
                if let Some(message) = selection {
                    if self.send(message.clone()) {
                        self.input.commit_selection(&message);
                    }
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
                if !matches!(hit, Some(WorkspaceHit::Terminal)) && workspace.is_some() {
                    self.terminal_scroll.cancel();
                    state.renderer.reset_cursor_animation();
                    state.window.request_redraw();
                }
                if matches!(hit, Some(WorkspaceHit::Tab(_))) {
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
        let now = Instant::now();
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
    snapshot: &eon_workspace_protocol::Snapshot,
    selected_endpoint: Option<&[u8]>,
    selected_attached: bool,
) -> HashSet<Vec<u8>> {
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

fn copy_is_ready(
    presentation_current: bool,
    selecting: bool,
    state: ElementState,
    repeat: bool,
) -> bool {
    presentation_current && !selecting && shortcut_is_ready(state, repeat)
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
) -> Option<WorkspaceAction> {
    use orbit_protocol::session::Modifiers;

    match (modifiers, code) {
        (Modifiers::ALT, KeyCode::KeyH) => Some(WorkspaceAction::Focus(WorkspaceDirection::Left)),
        (Modifiers::ALT, KeyCode::KeyL) => Some(WorkspaceAction::Focus(WorkspaceDirection::Right)),
        (Modifiers::ALT, KeyCode::KeyK) => Some(WorkspaceAction::Focus(WorkspaceDirection::Up)),
        (Modifiers::ALT, KeyCode::KeyJ) => Some(WorkspaceAction::Focus(WorkspaceDirection::Down)),
        (Modifiers::ALT, KeyCode::KeyM) => Some(WorkspaceAction::CreatePane),
        (Modifiers::CTRL, KeyCode::KeyT) => Some(WorkspaceAction::CreateTab),
        _ => None,
    }
}

fn sends_workspace_shortcut(action: &WorkspaceAction, state: ElementState, repeat: bool) -> bool {
    state == ElementState::Pressed && (!repeat || matches!(action, WorkspaceAction::Focus(_)))
}

fn clipboard_notice<E: std::fmt::Display>(result: std::result::Result<(), E>) -> String {
    result.map_or_else(
        |error| format!("Venus could not write the native clipboard: {error}"),
        |()| "Text copied to the native clipboard.".into(),
    )
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
    location: ClipboardLocation,
    text: String,
) -> std::result::Result<(), wl_clipboard_rs::copy::Error> {
    use wl_clipboard_rs::copy::{MimeType, Options, Source};

    let mut options = Options::new();
    options.clipboard(wayland_clipboard_type(location));
    options.copy(Source::Bytes(text.into_bytes().into()), MimeType::Text)
}

fn read_native_clipboard() -> std::result::Result<impl Read, String> {
    use wl_clipboard_rs::paste::{self, ClipboardType, MimeType, Seat};

    let (pipe, _) = paste::get_contents(ClipboardType::Regular, Seat::Unspecified, MimeType::Text)
        .map_err(|error| error.to_string())?;
    Ok(pipe)
}

fn wayland_clipboard_type(location: ClipboardLocation) -> wl_clipboard_rs::copy::ClipboardType {
    use wl_clipboard_rs::copy::ClipboardType;

    match location {
        ClipboardLocation::Standard => ClipboardType::Regular,
        ClipboardLocation::Selection | ClipboardLocation::Primary => ClipboardType::Primary,
    }
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
    if arguments.supervised {
        start_presentation_control(event_loop.create_proxy())?;
    }
    let mut application = Application::new(arguments, event_loop.create_proxy());
    event_loop.run_app(&mut application)?;
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
    use eon_workspace_protocol::{Pane, Snapshot, Tab};

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
        assert_eq!(scroll.next_request(7, Some(&preview), 20.0), None);
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
            scroll.next_request(8, None, 20.0),
            Some(ClientMessage::PreviewVertical {
                frame_revision: 8,
                direction: VerticalDirection::Up,
            })
        );

        let edge = ScenePreview::Viewport {
            frame_revision: 8,
            direction: VerticalDirection::Up,
            edge_reached: true,
            rows: Vec::new(),
        };
        scroll.preview_arrived(8, VerticalDirection::Up);
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
            active_tab: "tab-1".into(),
            tabs: vec![
                Tab {
                    id: "tab-1".into(),
                    selected_pane: "pane-1".into(),
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
                    id: "tab-2".into(),
                    selected_pane: "pane-3".into(),
                    panes: vec![Pane {
                        id: "pane-3".into(),
                        session: "session-3".into(),
                        endpoint: b"hidden".to_vec(),
                        live: true,
                    }],
                },
            ],
        };

        assert_eq!(
            visible_metadata_endpoints(&snapshot, Some(b"one"), false),
            HashSet::new()
        );
        assert_eq!(
            visible_metadata_endpoints(&snapshot, Some(b"one"), true),
            [b"one".to_vec()].into()
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
        assert!(!presentation.is_current(candidate));
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
                "orbit.sock",
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
    fn attachment_revision_collision_waits_for_the_new_generation() {
        let mut presentation = PresentationState::default();
        let attachment_a = presentation.candidate(Some(1));
        presentation.publish(attachment_a);
        assert_eq!(presentation.current_revision(attachment_a), Some(1));

        presentation.invalidate();
        let attachment_b = presentation.candidate(Some(1));
        assert_ne!(attachment_b, attachment_a);
        assert_eq!(presentation.current_revision(attachment_b), None);

        presentation.publish(attachment_b);
        assert_eq!(presentation.current_revision(attachment_b), Some(1));
    }

    #[test]
    fn workspace_generation_is_not_current_until_presented() {
        let mut presentation = PresentationState::default();
        let workspace_a = presentation.candidate(Some(7));
        presentation.publish(workspace_a);
        assert!(presentation.is_current(workspace_a));

        presentation.invalidate();
        let workspace_b = presentation.candidate(Some(7));
        assert!(!presentation.is_current(workspace_b));
        presentation.publish(workspace_a);
        assert!(!presentation.is_current(workspace_b));

        presentation.publish(workspace_b);
        assert!(presentation.is_current(workspace_b));
        presentation.unpublish();
        assert!(!presentation.is_current(workspace_b));

        presentation.publish(workspace_b);
        assert!(presentation.is_current(workspace_b));
    }

    #[test]
    fn published_accessibility_focus_waits_for_eon_queue_admission() {
        for (target, admitted, expected_focus, expected_action) in [
            (
                AccessibilityTarget::Tab("tab-2".into()),
                true,
                Some(WorkspaceFocus::Tabs),
                WorkspaceAction::FocusId("tab-2".into()),
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
    fn direct_workspace_shortcuts_use_existing_eon_actions() {
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
            (KeyCode::KeyT, Modifiers::CTRL, WorkspaceAction::CreateTab),
        ] {
            assert_eq!(workspace_shortcut(key, modifiers), Some(action));
        }
        assert_eq!(workspace_shortcut(KeyCode::KeyT, Modifiers::ALT), None);
        assert_eq!(
            workspace_shortcut(KeyCode::KeyH, Modifiers::ALT.union(Modifiers::SHIFT)),
            None
        );
    }

    #[test]
    fn creation_shortcuts_ignore_repeat_while_traversal_may_repeat() {
        assert!(!sends_workspace_shortcut(
            &WorkspaceAction::CreateTab,
            ElementState::Pressed,
            true
        ));
        assert!(sends_workspace_shortcut(
            &WorkspaceAction::Focus(WorkspaceDirection::Right),
            ElementState::Pressed,
            true
        ));
        assert!(!sends_workspace_shortcut(
            &WorkspaceAction::CreatePane,
            ElementState::Released,
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
            cell: orbit_protocol::session::ViewportCell { x: 0, y: 0 },
        });

        assert!(can_follow_implicit_resize(&mouse));
        assert!(!can_follow_implicit_resize(&selection));
    }

    #[test]
    fn copy_waits_for_current_presentation_and_selection_finish() {
        use winit::event::ElementState::{Pressed, Released};

        assert!(!copy_is_ready(false, false, Pressed, false));
        assert!(!copy_is_ready(true, true, Pressed, false));
        assert!(copy_is_ready(true, false, Pressed, false));
        assert!(!copy_is_ready(true, false, Pressed, true));
        assert!(!copy_is_ready(true, false, Released, false));
    }

    #[test]
    fn resize_does_not_dismiss_clipboard_result() {
        assert!(!dismisses_clipboard_notice(LocalNoticeSource::Resize));
        assert!(dismisses_clipboard_notice(LocalNoticeSource::Input));
    }

    #[test]
    fn clipboard_results_are_attributed_without_copying_terminal_cells() {
        assert_eq!(
            clipboard_notice::<&str>(Ok(())),
            "Text copied to the native clipboard."
        );
        assert_eq!(
            clipboard_notice(Err("display unavailable")),
            "Venus could not write the native clipboard: display unavailable"
        );
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
    fn terminal_clipboard_locations_use_wayland_targets() {
        use wl_clipboard_rs::copy::ClipboardType::{Primary, Regular};

        assert!(matches!(
            wayland_clipboard_type(ClipboardLocation::Standard),
            Regular
        ));
        assert!(matches!(
            wayland_clipboard_type(ClipboardLocation::Selection),
            Primary
        ));
        assert!(matches!(
            wayland_clipboard_type(ClipboardLocation::Primary),
            Primary
        ));
    }
}
