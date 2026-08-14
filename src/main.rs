#![forbid(unsafe_code)]

use accesskit::Action as AccessibilityAction;
use accesskit_winit::{Event as AccessKitEvent, WindowEvent as AccessKitWindowEvent};
use eon_workspace_protocol::{Action as WorkspaceAction, Direction as WorkspaceDirection};
use orbit_protocol::{
    MAX_CELLS,
    session::{
        self, ClientMessage, ClipboardLocation, FailureCode, SelectionAction, ServerMessage,
        SurfaceSize,
    },
};
use std::{
    env,
    error::Error,
    ffi::OsString,
    fs,
    io::{self, Read},
    os::unix::ffi::OsStringExt,
    os::unix::fs::MetadataExt,
    path::PathBuf,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
#[cfg(target_os = "linux")]
use winit::platform::wayland::{ActiveEventLoopExtWayland, WindowAttributesExtWayland};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalPosition, PhysicalSize},
    event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    keyboard::{KeyCode, PhysicalKey},
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::{UserAttentionType, Window, WindowAttributes, WindowId},
};
use yazelix_venus::{
    Accessibility, AccessibilityTarget, CellMetrics, Color, ConnectionState, InputState,
    LocalNoticeSource, PresentOutcome, Renderer, SessionModel, Transport, TransportEvent,
    WorkspaceEvent, WorkspaceFocus, WorkspaceHit, WorkspaceModel, WorkspaceScene,
    WorkspaceTransport,
};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;
const BLINK_INTERVAL: Duration = Duration::from_millis(500);
const ANIMATION_FRAME_INTERVAL: Duration = Duration::from_millis(16);
const INITIAL_RETRY_DELAY: Duration = Duration::from_millis(250);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(5);
const USAGE: &str = "usage: yazelix-venus [--no-decorations] [--background-opacity VALUE] [--background-blur] [--cursor-effect-v1 none|tail] [--cursor-trail-color-v1 #RRGGBB --cursor-trail-duration-v1 0.25..4.0] [ORBIT_SOCKET [EON_WORKSPACE_SOCKET]]";

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

#[derive(Debug)]
enum UserEvent {
    AccessKit(AccessKitEvent),
    Present,
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
    adapter: accesskit_winit::Adapter,
    accessibility: Accessibility,
    ime_line_offset: u16,
    window: Arc<Window>,
}

struct Application {
    orbit_socket: PathBuf,
    workspace_socket: Option<PathBuf>,
    decorations: bool,
    background_opacity: f32,
    background_blur: bool,
    cursor_tail: Option<(Color, f32)>,
    proxy: EventLoopProxy<UserEvent>,
    window: Option<WindowState>,
    transport: Option<Transport>,
    workspace_transport: Option<WorkspaceTransport>,
    model: SessionModel,
    workspace_model: WorkspaceModel,
    input: InputState,
    last_resize: Option<SurfaceSize>,
    presented_revision: Option<u64>,
    render_notice: Option<String>,
    blink_visible: bool,
    next_blink: Option<Instant>,
    next_animation: Option<Instant>,
    clipboard: Option<arboard::Clipboard>,
    active_endpoint: Option<Vec<u8>>,
    active_endpoint_live: bool,
    orbit_retry: OrbitRetry,
    retry_suppressed: bool,
    window_focused: bool,
    window_occluded: bool,
    workspace_focus: WorkspaceFocus,
    tab_scroll: f32,
    pane_scroll: f32,
    cursor: PhysicalPosition<f64>,
}

impl Application {
    fn new(
        orbit_socket: PathBuf,
        workspace_socket: Option<PathBuf>,
        decorations: bool,
        background_opacity: f32,
        background_blur: bool,
        cursor_tail: Option<(Color, f32)>,
        proxy: EventLoopProxy<UserEvent>,
    ) -> Self {
        Self {
            orbit_socket,
            workspace_socket,
            decorations,
            background_opacity,
            background_blur,
            cursor_tail,
            proxy,
            window: None,
            transport: None,
            workspace_transport: None,
            model: SessionModel::new(),
            workspace_model: WorkspaceModel::default(),
            input: InputState::default(),
            last_resize: None,
            presented_revision: None,
            render_notice: None,
            blink_visible: true,
            next_blink: None,
            next_animation: None,
            clipboard: None,
            active_endpoint: None,
            active_endpoint_live: false,
            orbit_retry: OrbitRetry::default(),
            retry_suppressed: false,
            window_focused: false,
            window_occluded: false,
            workspace_focus: WorkspaceFocus::Terminal,
            tab_scroll: 0.0,
            pane_scroll: 0.0,
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
        let attributes = if event_loop.is_wayland() {
            attributes.with_name("eon", "yazelix-venus")
        } else {
            attributes
        };
        let window = Arc::new(event_loop.create_window(attributes)?);
        // X11 ignores the exclusion size, so use the cursor's bottom edge as its spot.
        let ime_line_offset = u16::from(matches!(
            window.window_handle()?.as_raw(),
            RawWindowHandle::Xlib(_) | RawWindowHandle::Xcb(_)
        ));
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
        window.set_ime_allowed(true);
        window.set_visible(true);

        self.window = Some(WindowState {
            renderer,
            adapter,
            accessibility,
            ime_line_offset,
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

    fn workspace_scene(&self) -> Option<WorkspaceScene> {
        let state = self.window.as_ref()?;
        self.workspace_model.snapshot().map(|snapshot| {
            WorkspaceScene::from_snapshot(
                snapshot,
                state.renderer.size(),
                state.renderer.metrics(),
                self.tab_scroll,
                self.pane_scroll,
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
        self.workspace_focus = focus;
        let is_focused = terminal_focused(self.window_focused, self.workspace_focus);
        if was_focused != is_focused {
            let message = self.input.focus(is_focused);
            self.send(message);
        }
        self.refresh_client_view();
    }

    fn set_orbit_attachment(&mut self, endpoint: Vec<u8>, live: bool) {
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
        self.input.reset_scroll();
        self.input.cancel_selection();
        self.last_resize = None;
        self.presented_revision = None;
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
        self.reset_cursor_animation();
        self.retry_suppressed = false;
        self.last_resize = None;
        self.presented_revision = None;
        self.start_orbit(self.orbit_socket.clone());
        self.refresh_client_view();
    }

    fn handle_workspace(&mut self, event: WorkspaceEvent) {
        let (view_changed, snapshot_changed) = match event {
            WorkspaceEvent::Response(response) => self.workspace_model.apply(response),
            WorkspaceEvent::Unavailable(detail) => {
                (self.workspace_model.mark_unavailable(detail), false)
            }
        };
        if snapshot_changed {
            self.reveal_workspace_selection();
            if let Some((endpoint, live)) = self.workspace_model.active_attachment()
                && (self.active_endpoint.as_deref() != Some(endpoint)
                    || self.active_endpoint_live != live)
            {
                self.set_orbit_attachment(endpoint.to_vec(), live);
            }
        }
        if view_changed {
            self.refresh_client_view();
        }
    }

    fn send_workspace(&mut self, action: WorkspaceAction) {
        let Some(transport) = &self.workspace_transport else {
            return;
        };
        if let Err(error) = transport.send(action)
            && self
                .workspace_model
                .mark_unavailable(format!("Cannot queue Eon workspace action: {error}"))
        {
            self.refresh_client_view();
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
        if self
            .input
            .consumes_workspace_shortcut(code, event.state, shortcut.is_some())
        {
            if let Some(action) = shortcut
                .filter(|action| sends_workspace_shortcut(action, event.state, event.repeat))
            {
                self.send_workspace(action);
            }
            return true;
        }
        if self.workspace_focus == WorkspaceFocus::Terminal {
            return false;
        }
        if code == KeyCode::Escape {
            if event.state == ElementState::Pressed {
                self.set_workspace_focus(WorkspaceFocus::Terminal);
            }
            return true;
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
        if tabs {
            let movement = if horizontal == 0.0 {
                vertical
            } else {
                horizontal
            };
            self.tab_scroll = (scene.tab_scroll() - movement).clamp(0.0, scene.tab_scroll_limit());
        } else {
            self.pane_scroll =
                (scene.pane_scroll() - vertical).clamp(0.0, scene.pane_scroll_limit());
        }
        self.refresh_client_view();
    }

    fn handle_transport(&mut self, event: TransportEvent) {
        let transport_ended = matches!(
            &event,
            TransportEvent::RetryableLoss(_) | TransportEvent::Lost(_)
        );
        let retryable_loss = matches!(&event, TransportEvent::RetryableLoss(_))
            && !self.model.is_terminal()
            && !self.retry_suppressed;
        match event {
            TransportEvent::Server(message) => {
                if server_failure_suppresses_retry(&message) {
                    self.retry_suppressed = true;
                }
                let was_attached = self.model.is_attached();
                let frame = matches!(&message, ServerMessage::Frame(_));
                match self.model.apply(message) {
                    Ok(Some((location, text))) => self.write_clipboard(location, text),
                    Ok(None) => {}
                    Err(error) => self.model.mark_lost(error.to_string()),
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
                    self.input.reset_scroll();
                    self.input.cancel_selection();
                    self.send_resize();
                    if let Some(message) = self.input.latest_focus() {
                        self.send(message);
                    }
                }
            }
            TransportEvent::InvalidInput(detail) => {
                self.model.set_venus_notice(
                    LocalNoticeSource::Input,
                    format!("Venus could not encode input: {detail}"),
                );
            }
            TransportEvent::RetryableLoss(detail) => {
                self.input.reset_scroll();
                self.input.cancel_selection();
                if retryable_loss {
                    self.model.mark_lost(detail);
                }
            }
            TransportEvent::Lost(detail) => {
                self.input.reset_scroll();
                self.input.cancel_selection();
                self.model.mark_lost(detail);
            }
        }
        if transport_ended || self.model.is_terminal() {
            self.reset_cursor_animation();
            self.transport = None;
            self.presented_revision = None;
            if retryable_loss && self.retry_allowed() {
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
        let Some(state) = &mut self.window else {
            return;
        };
        if let Some(scene) = self.model.scene() {
            state.window.set_title(if scene.title.is_empty() {
                "Venus"
            } else {
                &scene.title
            });
            if let Some(cursor) = scene.cursor {
                let metrics = state.renderer.metrics();
                let origin = workspace.as_ref().map_or((0.0, 0.0), |workspace| {
                    (workspace.terminal.left, workspace.terminal.top)
                });
                let left =
                    origin.0 + metrics.padding + f32::from(cursor.leading_column()) * metrics.width;
                let top = origin.1
                    + metrics.padding
                    + f32::from(cursor.row + state.ime_line_offset) * metrics.height;
                state.window.set_ime_cursor_area(
                    PhysicalPosition::new(f64::from(left), f64::from(top)),
                    PhysicalSize::new(f64::from(metrics.width * 2.0), f64::from(metrics.height)),
                );
            }
        } else {
            state.window.set_title("Venus");
        }
        state
            .window
            .set_ime_allowed(self.workspace_focus == WorkspaceFocus::Terminal);
        state.accessibility.update(
            &mut state.adapter,
            self.model.scene(),
            workspace.as_ref(),
            self.workspace_focus,
            &status,
            state.renderer.size(),
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
        let result = self
            .clipboard()
            .and_then(|clipboard| write_native_clipboard(clipboard, location, text));
        self.model
            .set_venus_notice(LocalNoticeSource::Clipboard, clipboard_notice(result));
    }

    fn clipboard(&mut self) -> std::result::Result<&mut arboard::Clipboard, arboard::Error> {
        if self.clipboard.is_none() {
            self.clipboard = Some(arboard::Clipboard::new()?);
        }
        Ok(self
            .clipboard
            .as_mut()
            .expect("clipboard initialized above"))
    }

    fn paste_clipboard(&mut self) {
        if !self.model.is_attached() {
            return;
        }
        let result = self
            .clipboard()
            .map_err(|error| error.to_string())
            .and_then(|clipboard| clipboard_paste_message(clipboard.get_text()));
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
            ConnectionState::Attached { .. } if self.model.awaiting_current_frame() => {
                "Attached to Orbit. Waiting for its current frame.".into()
            }
            ConnectionState::Attached { .. } => String::new(),
            ConnectionState::Busy => "Orbit already has an attached presentation client.".into(),
            ConnectionState::Incompatible { minimum, maximum } => format!(
                "Orbit requires local-session revision {minimum} through {maximum}; Venus accepts revision {}.",
                session::VERSION
            ),
            ConnectionState::Lost { detail } => format!("Orbit connection lost: {detail}"),
            ConnectionState::Exited { code } => {
                format!("The authoritative Orbit process exited with status {code}.")
            }
        }
    }

    fn render(&mut self) {
        if !cursor_animation_allowed(self.window_focused, self.window_occluded) {
            self.reset_cursor_animation();
        }
        let status = self.status();
        let preedit = self.input.preedit();
        let workspace = self.workspace_scene();
        let mut refresh = false;
        let Some(state) = &mut self.window else {
            return;
        };
        match state.renderer.render(
            self.model.scene(),
            workspace.as_ref(),
            self.workspace_focus,
            &status,
            self.blink_visible,
            preedit,
        ) {
            Ok(PresentOutcome::Presented) => {
                refresh = self.render_notice.take().is_some();
                self.presented_revision = self.model.scene().and_then(|scene| {
                    let screen = terminal_screen(workspace.as_ref(), state.renderer.size());
                    surface_size(screen, state.renderer.metrics()).map(|_| scene.revision)
                });
            }
            Ok(PresentOutcome::Deferred) => {}
            Ok(PresentOutcome::Recovered) => {
                self.presented_revision = None;
                state.window.request_redraw();
            }
            Err(error) => {
                let notice = format!("Venus renderer failure: {error}");
                self.render_notice = Some(notice.clone());
                self.presented_revision = None;
                state.accessibility.update(
                    &mut state.adapter,
                    self.model.scene(),
                    workspace.as_ref(),
                    self.workspace_focus,
                    &notice,
                    state.renderer.size(),
                );
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
        .with_visible(false)
}

impl ApplicationHandler<UserEvent> for Application {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none()
            && let Err(error) = self.create_window(event_loop)
        {
            eprintln!("venus: {error}");
            event_loop.exit();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let presented_revision = current_presentation(
            self.model.scene().map(|scene| scene.revision),
            self.presented_revision,
        );
        let workspace = self.workspace_scene();
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
                state.renderer.resize(size, state.window.scale_factor());
                self.reveal_workspace_selection();
                self.presented_revision = None;
                self.send_resize();
                self.refresh_client_view();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.next_animation = None;
                self.input.reset_scroll();
                state
                    .renderer
                    .resize(state.window.inner_size(), scale_factor);
                self.reveal_workspace_selection();
                self.presented_revision = None;
                self.send_resize();
                self.refresh_client_view();
            }
            WindowEvent::Occluded(occluded) => {
                self.window_occluded = occluded;
                self.next_animation = None;
                state.renderer.reset_cursor_animation();
                if !occluded {
                    state.window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => self.render(),
            WindowEvent::ModifiersChanged(modifiers) => self.input.set_modifiers(modifiers.state()),
            WindowEvent::KeyboardInput { event, .. } => {
                if self.handle_workspace_key(&event) {
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
                        self.send(ClientMessage::Selection(SelectionAction::Copy));
                    }
                } else {
                    self.send(self.input.key(&event));
                }
            }
            WindowEvent::Ime(event) => {
                if ime_reaches_terminal(self.workspace_focus, &event) {
                    let message = self.input.ime(event);
                    if let Some(message) = message {
                        self.send(message);
                    }
                }
                self.refresh_client_view();
            }
            WindowEvent::Focused(focused) => {
                self.window_focused = focused;
                self.next_animation = None;
                state.renderer.reset_cursor_animation();
                let message = self
                    .input
                    .focus(terminal_focused(focused, self.workspace_focus));
                self.send(message);
                self.refresh_client_view();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = position;
                let (x, y) = workspace
                    .as_ref()
                    .map_or((position.x, position.y), |workspace| {
                        (
                            position.x - f64::from(workspace.terminal.left),
                            position.y - f64::from(workspace.terminal.top),
                        )
                    });
                let motion = self.input.move_pointer(x, y);
                if presented_revision.is_some() {
                    if self.input.is_selecting() {
                        let screen = terminal_screen(workspace.as_ref(), state.renderer.size());
                        let size = surface_size(screen, state.renderer.metrics());
                        if let Some(message) =
                            size.and_then(|size| self.input.selection_motion(size))
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
            }
            WindowEvent::MouseInput {
                state: button_state,
                button,
                ..
            } => {
                let renderer_size = state.renderer.size();
                let metrics = state.renderer.metrics();
                let hit = workspace.as_ref().and_then(|workspace| {
                    workspace.hit_test(self.cursor.x as f32, self.cursor.y as f32)
                });
                if button_state == ElementState::Pressed && button == MouseButton::Left {
                    let target = match hit {
                        Some(WorkspaceHit::Tab(id)) => {
                            self.set_workspace_focus(WorkspaceFocus::Tabs);
                            Some(id.to_owned())
                        }
                        Some(WorkspaceHit::Pane(id)) => {
                            self.set_workspace_focus(WorkspaceFocus::Panes);
                            Some(id.to_owned())
                        }
                        Some(WorkspaceHit::Terminal) => {
                            self.set_workspace_focus(WorkspaceFocus::Terminal);
                            None
                        }
                        None => None,
                    };
                    if let Some(id) = target {
                        self.send_workspace(WorkspaceAction::FocusId(id));
                        return;
                    }
                }
                if !button_reaches_terminal(
                    workspace.is_some(),
                    matches!(hit, Some(WorkspaceHit::Terminal)),
                    button_state,
                ) {
                    return;
                }
                if let Some(workspace) = &workspace {
                    let _ = self.input.move_pointer(
                        self.cursor.x - f64::from(workspace.terminal.left),
                        self.cursor.y - f64::from(workspace.terminal.top),
                    );
                }
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
                    && self.send(message)
                {
                    self.input.commit_mouse_button(button_state, button);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if self.input.is_selecting() {
                    return;
                }
                let metrics = state.renderer.metrics();
                let hit = workspace.as_ref().and_then(|workspace| {
                    workspace.hit_test(self.cursor.x as f32, self.cursor.y as f32)
                });
                if matches!(hit, Some(WorkspaceHit::Tab(_))) {
                    self.scroll_workspace(delta, true, metrics);
                    return;
                }
                if matches!(hit, Some(WorkspaceHit::Pane(_))) {
                    self.scroll_workspace(delta, false, metrics);
                    return;
                }
                if workspace.is_some() && !matches!(hit, Some(WorkspaceHit::Terminal)) {
                    return;
                }
                if presented_revision.is_none() {
                    return;
                }
                if let Some(workspace) = &workspace {
                    let _ = self.input.move_pointer(
                        self.cursor.x - f64::from(workspace.terminal.left),
                        self.cursor.y - f64::from(workspace.terminal.top),
                    );
                }
                for message in self.input.wheel(delta, metrics.width, metrics.height) {
                    if !self.send(message) {
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    fn user_event(&mut self, _: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Present => {
                if let Some(state) = &self.window {
                    state.window.set_minimized(false);
                    state.window.focus_window();
                    state
                        .window
                        .request_user_attention(Some(UserAttentionType::Informational));
                }
            }
            UserEvent::Transport => {
                let events = self
                    .transport
                    .as_ref()
                    .map_or_else(Vec::new, Transport::drain_events);
                for event in events {
                    self.handle_transport(event);
                }
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
                        match state.accessibility.workspace_target(request.target_node) {
                            Some(AccessibilityTarget::Terminal) => {
                                self.set_workspace_focus(WorkspaceFocus::Terminal);
                            }
                            Some(AccessibilityTarget::Tab(id)) => {
                                self.set_workspace_focus(WorkspaceFocus::Tabs);
                                self.send_workspace(WorkspaceAction::FocusId(id));
                            }
                            Some(AccessibilityTarget::Pane(id)) => {
                                self.set_workspace_focus(WorkspaceFocus::Panes);
                                self.send_workspace(WorkspaceAction::FocusId(id));
                            }
                            None => {}
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

fn server_failure_suppresses_retry(message: &ServerMessage) -> bool {
    matches!(
        message,
        ServerMessage::Failure(failure)
            if matches!(failure.code, FailureCode::Protocol | FailureCode::Terminal)
    )
}

fn current_presentation(
    scene_revision: Option<u64>,
    presented_revision: Option<u64>,
) -> Option<u64> {
    scene_revision.filter(|revision| Some(*revision) == presented_revision)
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

fn button_reaches_terminal(workspace: bool, terminal_hit: bool, state: ElementState) -> bool {
    !workspace || terminal_hit || state == ElementState::Released
}

fn ime_reaches_terminal(focus: WorkspaceFocus, event: &Ime) -> bool {
    focus == WorkspaceFocus::Terminal || matches!(event, Ime::Disabled)
}

fn terminal_focused(window_focused: bool, workspace_focus: WorkspaceFocus) -> bool {
    window_focused && workspace_focus == WorkspaceFocus::Terminal
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

fn clipboard_paste_message(
    text: std::result::Result<String, arboard::Error>,
) -> std::result::Result<ClientMessage, String> {
    let text = text.map_err(|error| error.to_string())?;
    if text.is_empty() {
        return Err("The native clipboard contains no text".into());
    }
    if text.len() > session::MAX_PASTE_BYTES {
        return Err(format!(
            "Native clipboard text exceeds the {} byte paste limit",
            session::MAX_PASTE_BYTES
        ));
    }
    Ok(ClientMessage::Paste(text.into_bytes()))
}

fn clipboard_paste_notice(error: impl std::fmt::Display) -> String {
    format!("Venus could not paste from the native clipboard: {error}")
}

fn write_native_clipboard(
    clipboard: &mut arboard::Clipboard,
    location: ClipboardLocation,
    text: String,
) -> std::result::Result<(), arboard::Error> {
    #[cfg(target_os = "linux")]
    {
        use arboard::SetExtLinux;

        clipboard
            .set()
            .clipboard(linux_clipboard_kind(location))
            .text(text)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = location;
        clipboard.set_text(text)
    }
}

#[cfg(target_os = "linux")]
fn linux_clipboard_kind(location: ClipboardLocation) -> arboard::LinuxClipboardKind {
    match location {
        ClipboardLocation::Standard => arboard::LinuxClipboardKind::Clipboard,
        ClipboardLocation::Selection | ClipboardLocation::Primary => {
            arboard::LinuxClipboardKind::Primary
        }
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

fn main() -> Result {
    let arguments = launch_arguments(env::args_os().skip(1))?;
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    if env::var_os("EON_VENUS_PRESENTATION_CONTROL") == Some(OsString::from("stdin")) {
        start_presentation_control(event_loop.create_proxy())?;
    }
    let mut application = Application::new(
        arguments.orbit_socket,
        arguments.workspace_socket,
        arguments.decorations,
        arguments.background_opacity,
        arguments.background_blur,
        arguments.cursor_tail,
        event_loop.create_proxy(),
    );
    event_loop.run_app(&mut application)?;
    Ok(())
}

#[derive(Debug)]
struct LaunchArguments {
    orbit_socket: PathBuf,
    workspace_socket: Option<PathBuf>,
    decorations: bool,
    background_opacity: f32,
    background_blur: bool,
    cursor_tail: Option<(Color, f32)>,
}

fn launch_arguments(arguments: impl IntoIterator<Item = OsString>) -> Result<LaunchArguments> {
    let mut arguments = arguments.into_iter();
    let mut orbit_socket = None;
    let mut workspace_socket = None;
    let mut decorations = true;
    let mut background_opacity = None;
    let mut background_blur = false;
    let mut cursor_effect = None;
    let mut cursor_trail_color = None;
    let mut cursor_trail_duration = None;

    while let Some(argument) = arguments.next() {
        if argument == "--no-decorations" {
            decorations = false;
        } else if argument == "--background-opacity" {
            if background_opacity.is_some() {
                return Err(USAGE.into());
            }
            let Some(value) = arguments
                .next()
                .and_then(|value| value.into_string().ok())
                .and_then(|value| value.parse::<f32>().ok())
                .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
            else {
                return Err(USAGE.into());
            };
            background_opacity = Some(value);
        } else if argument == "--background-blur" {
            if background_blur {
                return Err(USAGE.into());
            }
            background_blur = true;
        } else if argument == "--cursor-effect-v1" {
            if cursor_effect.is_some() {
                return Err(USAGE.into());
            }
            cursor_effect = match arguments.next().as_deref() {
                Some(value) if value == "none" => Some(false),
                Some(value) if value == "tail" => Some(true),
                _ => return Err(USAGE.into()),
            };
        } else if argument == "--cursor-trail-color-v1" {
            if cursor_trail_color.is_some() {
                return Err(USAGE.into());
            }
            cursor_trail_color = arguments
                .next()
                .and_then(|value| value.into_string().ok())
                .and_then(|value| parse_cursor_color(&value));
            if cursor_trail_color.is_none() {
                return Err(USAGE.into());
            }
        } else if argument == "--cursor-trail-duration-v1" {
            if cursor_trail_duration.is_some() {
                return Err(USAGE.into());
            }
            cursor_trail_duration = arguments
                .next()
                .and_then(|value| value.into_string().ok())
                .and_then(|value| value.parse::<f32>().ok())
                .filter(|value| value.is_finite() && (0.25..=4.0).contains(value));
            if cursor_trail_duration.is_none() {
                return Err(USAGE.into());
            }
        } else if argument.as_encoded_bytes().starts_with(b"-") {
            return Err(USAGE.into());
        } else if orbit_socket.is_none() {
            orbit_socket = Some(PathBuf::from(argument));
        } else if workspace_socket.is_none() {
            workspace_socket = Some(PathBuf::from(argument));
        } else {
            return Err(USAGE.into());
        }
    }

    let cursor_tail = match (cursor_effect, cursor_trail_color, cursor_trail_duration) {
        (None | Some(false), None, None) => None,
        (Some(true), Some(color), Some(duration)) => Some((color, duration)),
        _ => return Err(USAGE.into()),
    };

    Ok(LaunchArguments {
        orbit_socket: orbit_socket.map_or_else(default_socket_path, Ok)?,
        workspace_socket,
        decorations,
        background_opacity: background_opacity.unwrap_or(1.0),
        background_blur,
        cursor_tail,
    })
}

fn parse_cursor_color(value: &str) -> Option<Color> {
    let hex = value.strip_prefix('#')?;
    if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let rgb = u32::from_str_radix(hex, 16).ok()?;
    Some(Color {
        r: (rgb >> 16) as u8,
        g: (rgb >> 8) as u8,
        b: rgb as u8,
    })
}

fn start_presentation_control(proxy: EventLoopProxy<UserEvent>) -> Result {
    thread::Builder::new()
        .name("venus-presentation-control".into())
        .spawn(move || {
            let mut input = io::stdin().lock();
            let mut message = [0; 8];
            while input.read_exact(&mut message).is_ok() {
                if message == *b"present\n" && proxy.send_event(UserEvent::Present).is_err() {
                    break;
                }
            }
        })?;
    Ok(())
}

fn default_socket_path() -> Result<PathBuf> {
    if let Some(root) = env::var_os("XDG_RUNTIME_DIR") {
        return Ok(PathBuf::from(root).join("yazelix-orbit/orbit.sock"));
    }
    #[cfg(target_os = "linux")]
    let uid = fs::metadata("/proc/self")?.uid();
    #[cfg(not(target_os = "linux"))]
    let uid: u32 = env::var("UID")?.parse()?;
    Ok(PathBuf::from(format!(
        "/tmp/yazelix-orbit-{uid}/orbit.sock"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_launch_arguments_are_complete_bounded_and_default_static() {
        let parse = |arguments: &[&str]| launch_arguments(arguments.iter().map(OsString::from));

        let default = parse(&[]).unwrap();
        assert!(default.decorations && default.workspace_socket.is_none());
        assert_eq!(default.background_opacity, 1.0);
        assert!(!default.background_blur);
        assert_eq!(default.cursor_tail, None);
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
            ])
            .unwrap();
            assert!(!parsed.decorations);
            assert_eq!(parsed.background_opacity, value.parse::<f32>().unwrap());
            assert!(parsed.background_blur);
            assert_eq!(parsed.orbit_socket, PathBuf::from("orbit.sock"));
            assert_eq!(parsed.workspace_socket, Some(PathBuf::from("eon.sock")));
            let attributes = window_attributes(
                parsed.decorations,
                parsed.background_opacity,
                parsed.background_blur,
            );
            assert!(attributes.blur);
            assert_eq!(attributes.transparent, value != "1");
        }

        let tail = parse(&[
            "--cursor-effect-v1",
            "tail",
            "--cursor-trail-color-v1",
            "#12aBcF",
            "--cursor-trail-duration-v1",
            "2.5",
            "orbit.sock",
        ])
        .unwrap();
        assert_eq!(
            tail.cursor_tail,
            Some((
                yazelix_venus::Color {
                    r: 0x12,
                    g: 0xab,
                    b: 0xcf,
                },
                2.5,
            ))
        );
        assert_eq!(
            parse(&["--cursor-effect-v1", "none"]).unwrap().cursor_tail,
            None
        );

        for invalid in [
            &["--unknown"][..],
            &["one", "two", "three"][..],
            &["--background-opacity"][..],
            &["--background-opacity", "bad"][..],
            &["--background-opacity", "NaN"][..],
            &["--background-opacity", "inf"][..],
            &["--background-opacity", "-0.01"][..],
            &["--background-opacity", "1.01"][..],
            &["--background-opacity", "0.5", "--background-opacity", "0.6"][..],
            &["--background-blur", "--background-blur"][..],
            &["--cursor-effect-v1"][..],
            &["--cursor-effect-v1", "warp"][..],
            &["--cursor-effect-v1", "tail"][..],
            &[
                "--cursor-effect-v1",
                "tail",
                "--cursor-trail-color-v1",
                "#123456",
            ][..],
            &[
                "--cursor-effect-v1",
                "none",
                "--cursor-trail-color-v1",
                "#123456",
                "--cursor-trail-duration-v1",
                "1",
            ][..],
            &["--cursor-trail-color-v1", "#123456"][..],
            &["--cursor-trail-color-v1", "#aéabc"][..],
            &["--cursor-trail-color-v1", "123456"][..],
            &["--cursor-trail-color-v1", "#12345g"][..],
            &["--cursor-trail-duration-v1", "1"][..],
            &[
                "--cursor-effect-v1",
                "tail",
                "--cursor-trail-color-v1",
                "#123456",
                "--cursor-trail-duration-v1",
                "0.24",
            ][..],
            &[
                "--cursor-effect-v1",
                "tail",
                "--cursor-trail-color-v1",
                "#123456",
                "--cursor-trail-duration-v1",
                "4.01",
            ][..],
            &[
                "--cursor-effect-v1",
                "tail",
                "--cursor-trail-color-v1",
                "#123456",
                "--cursor-trail-duration-v1",
                "NaN",
            ][..],
            &[
                "--cursor-effect-v1",
                "tail",
                "--cursor-effect-v1",
                "tail",
                "--cursor-trail-color-v1",
                "#123456",
                "--cursor-trail-duration-v1",
                "1",
            ][..],
            &[
                "--cursor-effect-v1",
                "tail",
                "--cursor-trail-color-v1",
                "#123456",
                "--cursor-trail-color-v1",
                "#abcdef",
                "--cursor-trail-duration-v1",
                "1",
            ][..],
            &[
                "--cursor-effect-v1",
                "tail",
                "--cursor-trail-color-v1",
                "#123456",
                "--cursor-trail-duration-v1",
                "1",
                "--cursor-trail-duration-v1",
                "2",
            ][..],
        ] {
            assert_eq!(parse(invalid).unwrap_err().to_string(), USAGE);
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
    fn pointer_input_requires_the_current_presented_revision() {
        assert_eq!(current_presentation(None, None), None);
        assert_eq!(current_presentation(Some(8), None), None);
        assert_eq!(current_presentation(Some(8), Some(7)), None);
        assert_eq!(current_presentation(Some(8), Some(8)), Some(8));
    }

    #[test]
    fn workspace_chrome_preserves_terminal_release_pairing() {
        assert!(!button_reaches_terminal(true, false, ElementState::Pressed));
        assert!(button_reaches_terminal(true, false, ElementState::Released));
        assert!(button_reaches_terminal(true, true, ElementState::Pressed));
        assert!(button_reaches_terminal(false, false, ElementState::Pressed));
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
    fn ime_disable_cleanup_reaches_input_outside_terminal() {
        assert!(ime_reaches_terminal(WorkspaceFocus::Tabs, &Ime::Disabled));
        assert!(!ime_reaches_terminal(
            WorkspaceFocus::Panes,
            &Ime::Commit("ignored".into())
        ));
        assert!(ime_reaches_terminal(
            WorkspaceFocus::Terminal,
            &Ime::Commit("accepted".into())
        ));
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
    fn copy_waits_for_selection_to_finish() {
        use winit::event::ElementState::{Pressed, Released};

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
            clipboard_paste_message(Ok(text.into())),
            Ok(ClientMessage::Paste(text.as_bytes().to_vec()))
        );
        assert_eq!(
            clipboard_paste_message(Ok(String::new())).unwrap_err(),
            "The native clipboard contains no text"
        );
        assert_eq!(
            clipboard_paste_message(Ok("x".repeat(session::MAX_PASTE_BYTES + 1))).unwrap_err(),
            format!(
                "Native clipboard text exceeds the {} byte paste limit",
                session::MAX_PASTE_BYTES
            )
        );
        assert_eq!(
            clipboard_paste_message(Err(arboard::Error::ContentNotAvailable)).unwrap_err(),
            arboard::Error::ContentNotAvailable.to_string()
        );
        assert_eq!(
            clipboard_paste_notice("display unavailable"),
            "Venus could not paste from the native clipboard: display unavailable"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn terminal_clipboard_locations_use_the_existing_linux_targets() {
        use arboard::LinuxClipboardKind::{Clipboard, Primary};

        assert!(matches!(
            linux_clipboard_kind(ClipboardLocation::Standard),
            Clipboard
        ));
        assert!(matches!(
            linux_clipboard_kind(ClipboardLocation::Selection),
            Primary
        ));
        assert!(matches!(
            linux_clipboard_kind(ClipboardLocation::Primary),
            Primary
        ));
    }
}
