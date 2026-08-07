#![forbid(unsafe_code)]

use accesskit_winit::{Event as AccessKitEvent, WindowEvent as AccessKitWindowEvent};
use orbit_protocol::{
    MAX_CELLS,
    session::{self, ClientMessage, SelectionAction, ServerMessage, SurfaceSize},
};
use std::{
    env,
    error::Error,
    fs,
    os::unix::fs::MetadataExt,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalPosition, PhysicalSize},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::{Window, WindowId},
};
use yazelix_venus::{
    Accessibility, CellMetrics, ConnectionState, InputState, LocalNoticeSource, PresentOutcome,
    Renderer, SessionModel, Transport, TransportEvent,
};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;
const BLINK_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug)]
enum UserEvent {
    AccessKit(AccessKitEvent),
    Transport,
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
    window: Arc<Window>,
}

struct Application {
    socket: PathBuf,
    proxy: EventLoopProxy<UserEvent>,
    window: Option<WindowState>,
    transport: Option<Transport>,
    model: SessionModel,
    input: InputState,
    last_resize: Option<SurfaceSize>,
    presented_revision: Option<u64>,
    render_notice: Option<String>,
    blink_visible: bool,
    next_blink: Option<Instant>,
    clipboard: Option<arboard::Clipboard>,
}

impl Application {
    fn new(socket: PathBuf, proxy: EventLoopProxy<UserEvent>) -> Self {
        Self {
            socket,
            proxy,
            window: None,
            transport: None,
            model: SessionModel::new(),
            input: InputState::default(),
            last_resize: None,
            presented_revision: None,
            render_notice: None,
            blink_visible: true,
            next_blink: None,
            clipboard: None,
        }
    }

    fn create_window(&mut self, event_loop: &ActiveEventLoop) -> Result {
        let attributes = Window::default_attributes()
            .with_title("Venus")
            .with_inner_size(LogicalSize::new(960.0, 600.0))
            .with_visible(false);
        let window = Arc::new(event_loop.create_window(attributes)?);
        let accessibility = Accessibility::new(window.inner_size());
        let adapter = accesskit_winit::Adapter::with_mixed_handlers(
            event_loop,
            &window,
            accessibility.activation(),
            self.proxy.clone(),
        );
        let renderer = pollster::block_on(Renderer::new(Arc::clone(&window), event_loop))?;
        window.set_ime_allowed(true);
        window.set_visible(true);

        let proxy = self.proxy.clone();
        self.transport = Some(Transport::start(self.socket.clone(), move || {
            let _ = proxy.send_event(UserEvent::Transport);
        }));
        self.window = Some(WindowState {
            renderer,
            adapter,
            accessibility,
            window,
        });
        self.refresh_client_view();
        Ok(())
    }

    fn handle_transport(&mut self, event: TransportEvent) {
        match event {
            TransportEvent::Server(message) => {
                let was_attached = self.model.is_attached();
                let rejected = matches!(&message, ServerMessage::Failure(_));
                match self.model.apply(message) {
                    Ok(Some(text)) => self.write_clipboard(text),
                    Ok(None) => {}
                    Err(error) => self.model.mark_lost(error.to_string()),
                }
                if rejected {
                    self.input.cancel_selection();
                }
                if !was_attached && self.model.is_attached() {
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
            TransportEvent::Lost(detail) => {
                self.input.reset_scroll();
                self.input.cancel_selection();
                self.model.mark_lost(detail);
            }
        }
        if self.model.is_terminal() {
            self.transport = None;
        }
        self.refresh_client_view();
    }

    fn refresh_client_view(&mut self) {
        let status = self.status();
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
                state.window.set_ime_cursor_area(
                    PhysicalPosition::new(
                        f64::from(metrics.padding + f32::from(cursor.column) * metrics.width),
                        f64::from(metrics.padding + f32::from(cursor.row + 1) * metrics.height),
                    ),
                    PhysicalSize::new(f64::from(metrics.width), f64::from(metrics.height)),
                );
            }
        } else {
            state.window.set_title("Venus");
        }
        state.accessibility.update(
            &mut state.adapter,
            self.model.scene(),
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
        ) && let Some(size) = self
            .window
            .as_ref()
            .and_then(|state| surface_size(state.renderer.size(), state.renderer.metrics()))
            && self.last_resize != Some(size)
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
        }
        let queue_recovered = self.model.clear_venus_notice(LocalNoticeSource::Queue);
        let clipboard_cleared = dismisses_clipboard_notice(notice_source)
            && self.model.clear_venus_notice(LocalNoticeSource::Clipboard);
        if self.model.clear_venus_notice(notice_source) || queue_recovered || clipboard_cleared {
            self.refresh_client_view();
        }
        true
    }

    fn write_clipboard(&mut self, text: String) {
        let result = (|| {
            if self.clipboard.is_none() {
                self.clipboard = Some(arboard::Clipboard::new()?);
            }
            self.clipboard
                .as_mut()
                .expect("clipboard initialized above")
                .set_text(text)
        })();
        self.model
            .set_venus_notice(LocalNoticeSource::Clipboard, clipboard_notice(result));
    }

    fn send_resize(&mut self) {
        let Some(state) = &self.window else {
            return;
        };
        let Some(size) = surface_size(state.renderer.size(), state.renderer.metrics()) else {
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
        if let Some(notice) = self.model.notice() {
            return notice.to_owned();
        }
        match self.model.connection() {
            ConnectionState::Connecting => {
                format!("Connecting to Orbit at {}", self.socket.display())
            }
            ConnectionState::Attached { .. } if self.model.scene().is_none() => {
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
        let status = self.status();
        let preedit = self.input.preedit();
        let mut refresh = false;
        let Some(state) = &mut self.window else {
            return;
        };
        match state
            .renderer
            .render(self.model.scene(), &status, self.blink_visible, preedit)
        {
            Ok(PresentOutcome::Presented) => {
                refresh = self.render_notice.take().is_some();
                self.presented_revision = self.model.scene().and_then(|scene| {
                    surface_size(state.renderer.size(), state.renderer.metrics())
                        .map(|_| scene.revision)
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
        let Some(state) = &mut self.window else {
            return;
        };
        if state.window.id() != window_id {
            return;
        }
        state.adapter.process_event(&state.window, &event);

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                self.input.reset_scroll();
                self.input.cancel_selection();
                state.renderer.resize(size, state.window.scale_factor());
                self.presented_revision = None;
                self.send_resize();
                self.refresh_client_view();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.input.reset_scroll();
                self.input.cancel_selection();
                state
                    .renderer
                    .resize(state.window.inner_size(), scale_factor);
                self.presented_revision = None;
                self.send_resize();
                self.refresh_client_view();
            }
            WindowEvent::Occluded(false) => state.window.request_redraw(),
            WindowEvent::RedrawRequested => self.render(),
            WindowEvent::ModifiersChanged(modifiers) => self.input.set_modifiers(modifiers.state()),
            WindowEvent::KeyboardInput { event, .. } => {
                if self
                    .input
                    .consumes_copy_shortcut(event.physical_key, event.state, event.repeat)
                {
                    if copy_is_ready(self.input.is_selecting(), event.state, event.repeat) {
                        self.send(ClientMessage::Selection(SelectionAction::Copy));
                    }
                } else {
                    self.send(self.input.key(&event));
                }
            }
            WindowEvent::Ime(event) => {
                let message = self.input.ime(event);
                if let Some(message) = message {
                    self.send(message);
                }
                self.refresh_client_view();
            }
            WindowEvent::Focused(focused) => {
                let message = self.input.focus(focused);
                self.send(message);
                self.refresh_client_view();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let motion = self.input.move_pointer(position.x, position.y);
                if presented_revision.is_some() {
                    if self.input.is_selecting() {
                        let size = surface_size(state.renderer.size(), state.renderer.metrics());
                        if let Some(message) =
                            size.and_then(|size| self.input.selection_motion(size))
                            && self.send(message.clone())
                        {
                            self.input.commit_selection(&message);
                        }
                    } else if let Some(message) = motion {
                        self.send(message);
                    }
                }
            }
            WindowEvent::MouseInput {
                state: button_state,
                button,
                ..
            } => {
                let size = surface_size(state.renderer.size(), state.renderer.metrics());
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
            WindowEvent::MouseWheel { delta, .. } if presented_revision.is_some() => {
                let metrics = state.renderer.metrics();
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
            UserEvent::Transport => {
                let events = self
                    .transport
                    .as_ref()
                    .map_or_else(Vec::new, Transport::drain_events);
                for event in events {
                    self.handle_transport(event);
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
                    AccessKitWindowEvent::ActionRequested(_)
                    | AccessKitWindowEvent::AccessibilityDeactivated => {}
                }
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let blinking = self
            .model
            .scene()
            .is_some_and(|scene| scene.has_blinking_content());
        if update_blink(
            blinking,
            &mut self.blink_visible,
            &mut self.next_blink,
            Instant::now(),
        ) && let Some(state) = &self.window
        {
            state.window.request_redraw();
        }
        event_loop.set_control_flow(
            self.next_blink
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

fn current_presentation(
    scene_revision: Option<u64>,
    presented_revision: Option<u64>,
) -> Option<u64> {
    scene_revision.filter(|revision| Some(*revision) == presented_revision)
}

fn can_follow_implicit_resize(message: &ClientMessage) -> bool {
    matches!(message, ClientMessage::Mouse(_))
}

fn copy_is_ready(selecting: bool, state: winit::event::ElementState, repeat: bool) -> bool {
    !selecting && state == winit::event::ElementState::Pressed && !repeat
}

fn dismisses_clipboard_notice(source: LocalNoticeSource) -> bool {
    source == LocalNoticeSource::Input
}

fn clipboard_notice<E: std::fmt::Display>(result: std::result::Result<(), E>) -> String {
    result.map_or_else(
        |error| format!("Venus could not write the native clipboard: {error}"),
        |()| "Selection copied to the native clipboard.".into(),
    )
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
    let socket = socket_argument()?;
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    let mut application = Application::new(socket, event_loop.create_proxy());
    event_loop.run_app(&mut application)?;
    Ok(())
}

fn socket_argument() -> Result<PathBuf> {
    let mut arguments = env::args_os().skip(1);
    let socket = arguments.next().map(PathBuf::from);
    if arguments.next().is_some() {
        return Err("usage: yazelix-venus [SOCKET]".into());
    }
    socket.map_or_else(default_socket_path, Ok)
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
    fn surface_measurements_match_orbit_invariants() {
        let metrics = CellMetrics::for_scale(1.0);
        let size = surface_size(PhysicalSize::new(960, 600), metrics).unwrap();
        assert_eq!((size.cols, size.rows), (104, 32));
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
    fn pointer_input_requires_the_current_presented_revision() {
        assert_eq!(current_presentation(None, None), None);
        assert_eq!(current_presentation(Some(8), None), None);
        assert_eq!(current_presentation(Some(8), Some(7)), None);
        assert_eq!(current_presentation(Some(8), Some(8)), Some(8));
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
            "Selection copied to the native clipboard."
        );
        assert_eq!(
            clipboard_notice(Err("display unavailable")),
            "Venus could not write the native clipboard: display unavailable"
        );
    }
}
