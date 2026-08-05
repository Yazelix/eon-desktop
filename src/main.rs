#![forbid(unsafe_code)]

use accesskit_winit::{Event as AccessKitEvent, WindowEvent as AccessKitWindowEvent};
use orbit_protocol::{
    MAX_CELLS,
    session::{ClientMessage, SurfaceSize},
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
    event::{ElementState, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::{Window, WindowId},
};
use yazelix_venus::{
    Accessibility, CellMetrics, ConnectionState, InputState, LocalNoticeSource, PresentOutcome,
    Renderer, SessionModel, Transport, TransportEvent,
};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

#[derive(Debug)]
enum UserEvent {
    AccessKit(AccessKitEvent),
    Transport(TransportEvent),
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
    scene_presented: bool,
    render_notice: Option<String>,
    blink_visible: bool,
    next_blink: Instant,
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
            scene_presented: false,
            render_notice: None,
            blink_visible: true,
            next_blink: Instant::now() + Duration::from_millis(500),
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
        self.transport = Some(Transport::start(self.socket.clone(), move |event| {
            let _ = proxy.send_event(UserEvent::Transport(event));
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
                if let Err(error) = self.model.apply(message) {
                    self.model.mark_lost(error.to_string());
                }
                if !was_attached && self.model.is_attached() {
                    self.send_resize();
                }
            }
            TransportEvent::InvalidInput(detail) => {
                self.model.set_venus_notice(
                    LocalNoticeSource::Input,
                    format!("Venus could not encode input: {detail}"),
                );
            }
            TransportEvent::Lost(detail) => {
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
        let notice_source = if matches!(&message, ClientMessage::Resize(_)) {
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
        let queue_recovered = self.model.clear_venus_notice(LocalNoticeSource::Queue);
        if self.model.clear_venus_notice(notice_source) || queue_recovered {
            self.refresh_client_view();
        }
        true
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
        if self.send(ClientMessage::Resize(size)) {
            self.last_resize = Some(size);
        }
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
                "Orbit requires local-session revision {minimum} through {maximum}; Venus accepts revision 1."
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
                self.scene_presented = self.model.scene().is_some()
                    && surface_size(state.renderer.size(), state.renderer.metrics()).is_some();
            }
            Ok(PresentOutcome::Deferred) => {}
            Ok(PresentOutcome::Recovered) => {
                self.scene_presented = false;
                state.window.request_redraw();
            }
            Err(error) => {
                let notice = format!("Venus renderer failure: {error}");
                self.render_notice = Some(notice.clone());
                self.scene_presented = false;
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
                state.renderer.resize(size, state.window.scale_factor());
                self.scene_presented = false;
                self.send_resize();
                self.refresh_client_view();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                state
                    .renderer
                    .resize(state.window.inner_size(), scale_factor);
                self.scene_presented = false;
                self.send_resize();
                self.refresh_client_view();
            }
            WindowEvent::Occluded(false) => state.window.request_redraw(),
            WindowEvent::RedrawRequested => self.render(),
            WindowEvent::ModifiersChanged(modifiers) => self.input.set_modifiers(modifiers.state()),
            WindowEvent::KeyboardInput { event, .. } => {
                self.send(self.input.key(&event));
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
                if let Some(message) = self.input.move_pointer(position.x, position.y)
                    && self.scene_presented
                {
                    self.send(message);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(message) = self.input.mouse_button(state, button, self.scene_presented)
                    && !self.send(message)
                    && state == ElementState::Pressed
                {
                    self.input.reject_mouse_press(button);
                }
            }
            WindowEvent::MouseWheel { delta, .. } if self.scene_presented => {
                let (horizontal, vertical) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x, y),
                    MouseScrollDelta::PixelDelta(position) => {
                        (position.x as f32, position.y as f32)
                    }
                };
                if let Some(message) = self.input.wheel(horizontal, vertical) {
                    self.send(message);
                }
            }
            _ => {}
        }
    }

    fn user_event(&mut self, _: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Transport(event) => self.handle_transport(event),
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
        if !blinking {
            self.blink_visible = true;
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }

        let now = Instant::now();
        if now >= self.next_blink {
            self.blink_visible = !self.blink_visible;
            self.next_blink = now + Duration::from_millis(500);
            if let Some(state) = &self.window {
                state.window.request_redraw();
            }
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_blink));
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
    let columns = ((screen.width - horizontal_padding) / cell_width).max(1);
    let mut rows = ((screen.height - vertical_padding) / cell_height).max(1);
    rows = rows.min(MAX_CELLS as u32 / columns.max(1));
    Some(SurfaceSize {
        cols: u16::try_from(columns).ok()?,
        rows: u16::try_from(rows).ok()?,
        screen_width: screen.width,
        screen_height: screen.height,
        cell_width,
        cell_height,
        padding_top: padding,
        padding_bottom: padding,
        padding_left: padding,
        padding_right: padding,
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
        assert!(usize::from(size.cols) * usize::from(size.rows) <= MAX_CELLS);
        assert!(surface_size(PhysicalSize::new(1, 1), metrics).is_none());
        assert!(surface_size(PhysicalSize::new(u32::from(u16::MAX) + 1, 600), metrics).is_none());
    }
}
