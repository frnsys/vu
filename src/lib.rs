mod anim;
mod img;
mod view;

use std::path::Path;

use anim::{Animator, RequestNextFrame};
use view::ImageView;
use winit::{
    event::{ElementState, Event, KeyEvent, WindowEvent},
    event_loop::{EventLoopBuilder, EventLoopWindowTarget},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowBuilder, WindowLevel},
};

pub fn run(title: &str, image_path: &Path, max_side: Option<u32>) -> anyhow::Result<()> {
    let event_loop = EventLoopBuilder::<RequestNextFrame>::with_user_event()
        .build()
        .expect("Failed to create event loop");

    // Note: For Wayland positioning has to happen via the window manager.
    let window = WindowBuilder::new()
        .with_title(title)
        .with_resizable(true)
        .with_decorations(false)
        .with_window_level(WindowLevel::AlwaysOnTop)
        .build(&event_loop)
        .unwrap();

    let mut image_view = ImageView::new(image_path, &window, max_side)?;
    let _animator = image_view
        .image
        .delays()
        .map(|delays| Animator::new(&event_loop, delays));

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

            _ => handle_event(&window, &mut image_view, event, target),
        }
    })?;

    Ok(())
}

fn handle_event(
    window: &Window,
    image: &mut ImageView,
    event: Event<RequestNextFrame>,
    target: &EventLoopWindowTarget<RequestNextFrame>,
) {
    match event {
        // Exit on Esc or Q
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
            KeyCode::KeyF => {
                toggle_fullscreen(window);
            }
            KeyCode::KeyE => {
                image.zoom_in();
            }
            KeyCode::KeyH => {
                image.zoom_out();
            }
            KeyCode::ArrowUp => {
                image.pan_up();
            }
            KeyCode::ArrowDown => {
                image.pan_down();
            }
            KeyCode::ArrowRight => {
                image.pan_right();
            }
            KeyCode::ArrowLeft => {
                image.pan_left();
            }
            KeyCode::Escape | KeyCode::KeyQ => {
                target.exit();
            }
            _ => (),
        },
        Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } => target.exit(),
        _ => (),
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
