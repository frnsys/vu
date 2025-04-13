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

pub fn run(title: &str, should_focus: bool, image_path: &Path) -> anyhow::Result<()> {
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

    let mut image_view = ImageView::new(image_path, &window)?;
    let _animator = image_view
        .image
        .delays()
        .map(|delays| Animator::new(&event_loop, delays));

    event_loop.run(move |event, target| {
        match event {
            Event::WindowEvent {
                event: WindowEvent::Focused(true),
                ..
            } => {
                // This is a super-hacky, river-specific way
                // of making the window unfocusable. Basically
                // it punts focus back to whatever previously had focus.
                // Ideally `winit` gets some better support for Wayland features;
                // for example, in `sema` I use the `gtk` crate which lets you
                // make a window unfocusable; I just don't want to port everything
                // to that and this works ok for now.
                if !should_focus {
                    std::process::Command::new("riverctl")
                        .args(["focus-view", "previous"])
                        .output()
                        .unwrap();
                }
            }

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

            _ => handle_event(&window, event, target),
        }
    })?;

    Ok(())
}

fn handle_event(
    window: &Window,
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
