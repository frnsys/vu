mod anim;
mod img;
mod view;

use std::path::Path;

use anim::{Animator, RequestNextFrame};
use view::{ImageView, ViewOpts};
use winit::{
    event::{ElementState, Event, KeyEvent, WindowEvent},
    event_loop::{EventLoopBuilder, EventLoopProxy},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowBuilder, WindowLevel},
};

struct Viewer {
    view: ImageView,

    #[allow(unused)]
    animator: Option<Animator>,
}
impl Viewer {
    fn new(
        path: &Path,
        window: &Window,
        proxy: &EventLoopProxy<RequestNextFrame>,
        opts: ViewOpts,
    ) -> anyhow::Result<Self> {
        let view = ImageView::new(path, window, opts)?;
        let animator = view
            .image
            .delays()
            .map(|delays| Animator::new(proxy.clone(), delays));
        Ok(Self { view, animator })
    }
}
impl std::ops::DerefMut for Viewer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.view
    }
}
impl std::ops::Deref for Viewer {
    type Target = ImageView;

    fn deref(&self) -> &Self::Target {
        &self.view
    }
}

pub fn run<P: AsRef<Path>>(
    title: &str,
    image_paths: &[P],
    max_side: Option<u32>,
) -> anyhow::Result<()> {
    if let Some(image_path) = image_paths.first() {
        let event_loop = EventLoopBuilder::<RequestNextFrame>::with_user_event()
            .build()
            .expect("Failed to create event loop");
        let proxy = event_loop.create_proxy();

        // Note: For Wayland positioning has to happen via the window manager.
        let window = WindowBuilder::new()
            .with_title(title)
            .with_resizable(true)
            .with_decorations(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .build(&event_loop)
            .unwrap();

        let mut index: usize = 0;
        let label = format!(
            "{} {}/{}",
            image_path.as_ref().display(),
            index + 1,
            image_paths.len()
        );

        let mut image_view = Viewer::new(
            image_path.as_ref(),
            &window,
            &proxy,
            ViewOpts {
                max_side,
                resize_window: true,
                show_label: false,
                label,
            },
        )?;

        event_loop.run(move |event, target| {
            match event {
                // Go to the next frame in a sequence.
                Event::UserEvent(RequestNextFrame) => {
                    if !image_view.advance() {
                        target.exit();
                    }
                }
                Event::WindowEvent {
                    event: WindowEvent::RedrawRequested,
                    ..
                } => {
                    if !image_view.draw() {
                        target.exit();
                    }
                }
                Event::WindowEvent {
                    event: WindowEvent::Resized(size),
                    ..
                } => {
                    image_view.resize(size.width, size.height, true).unwrap();
                }

                _ => {
                    if let Some(action) = handle_event(event) {
                        match action {
                            Action::ToggleFullscreen => toggle_fullscreen(&window),
                            Action::ZoomIn => image_view.zoom_in(),
                            Action::ZoomOut => image_view.zoom_out(),
                            Action::PanUp => image_view.pan_up(),
                            Action::PanDown => image_view.pan_down(),
                            Action::PanRight => image_view.pan_right(),
                            Action::PanLeft => image_view.pan_left(),
                            Action::ToggleInfo => image_view.toggle_label(),
                            Action::ChangeImage(next) => {
                                index = if next {
                                    if index >= image_paths.len() - 1 {
                                        0
                                    } else {
                                        index + 1
                                    }
                                } else {
                                    if index == 0 {
                                        image_paths.len() - 1
                                    } else {
                                        index.saturating_sub(1)
                                    }
                                };
                                if let Some(image_path) = image_paths.get(index) {
                                    let label = format!(
                                        "{} {}/{}",
                                        image_path.as_ref().display(),
                                        index + 1,
                                        image_paths.len()
                                    );

                                    // Because by this point the WM has already positioned
                                    // the window, it's better to resize the image to the window
                                    // rather than vice-versa, because otherwise the window
                                    // positioning could get messed up.
                                    let view = Viewer::new(
                                        image_path.as_ref(),
                                        &window,
                                        &proxy,
                                        ViewOpts {
                                            max_side,
                                            resize_window: false,
                                            show_label: image_view.is_label_visible(),
                                            label,
                                        },
                                    );
                                    match view {
                                        Ok(view) => image_view = view,
                                        Err(err) => eprintln!("Error loading image: {err}"),
                                    }
                                } else {
                                    eprintln!("No image found for index {index}")
                                }
                            }
                            Action::Quit => target.exit(),
                        }
                    }
                }
            }
        })?;
    }
    Ok(())
}

enum Action {
    ToggleFullscreen,
    ZoomIn,
    ZoomOut,
    PanUp,
    PanDown,
    PanRight,
    PanLeft,
    ToggleInfo,
    ChangeImage(bool),
    Quit,
}

fn handle_event(event: Event<RequestNextFrame>) -> Option<Action> {
    match event {
        Event::WindowEvent {
            event:
                WindowEvent::KeyboardInput {
                    event:
                        KeyEvent {
                            state: ElementState::Pressed,
                            physical_key: PhysicalKey::Code(key),
                            ..
                        },
                    ..
                },
            ..
        } => match key {
            KeyCode::KeyF => Some(Action::ToggleFullscreen),
            KeyCode::KeyE => Some(Action::ZoomIn),
            KeyCode::KeyH => Some(Action::ZoomOut),
            KeyCode::ArrowUp => Some(Action::PanUp),
            KeyCode::ArrowDown => Some(Action::PanDown),
            KeyCode::ArrowRight => Some(Action::PanRight),
            KeyCode::ArrowLeft => Some(Action::PanLeft),
            KeyCode::Quote => Some(Action::ChangeImage(true)),
            KeyCode::Comma => Some(Action::ChangeImage(false)),
            KeyCode::KeyI => Some(Action::ToggleInfo),
            KeyCode::Escape | KeyCode::KeyQ => Some(Action::Quit),
            _ => None,
        },
        Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } => Some(Action::Quit),
        _ => None,
    }
}

fn toggle_fullscreen(window: &Window) {
    if window.fullscreen().is_some() {
        window.set_fullscreen(None);
    } else {
        let monitor = window.current_monitor().unwrap();
        let fullscreen = winit::window::Fullscreen::Borderless(Some(monitor));
        window.set_fullscreen(Some(fullscreen));
    }
}
