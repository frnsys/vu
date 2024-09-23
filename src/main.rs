mod img;

use std::{
    path::PathBuf,
    process::exit,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use pixels::{wgpu::Color, Error, PixelsBuilder, SurfaceTexture};
use winit::{
    dpi::{LogicalSize, PhysicalSize},
    event::{Event, KeyEvent, WindowEvent},
    event_loop::{EventLoop, EventLoopBuilder},
    keyboard::{KeyCode, PhysicalKey},
    window::{WindowBuilder, WindowLevel},
};

/// Minimum margin around images.
const MARGIN: u32 = 50;

/// Transparent/background color.
const CLEAR_COLOR: Color = Color::BLUE;

/// Event emitted when the next frame in
/// an image sequence should be shown.
#[derive(Debug)]
struct RequestNextFrame;

#[derive(Debug)]
struct Args {
    path: PathBuf,
    title: String,
    no_focus: bool,
}
impl Args {
    fn parse() -> Result<Self, pico_args::Error> {
        let mut pargs = pico_args::Arguments::from_env();
        let args = Self {
            path: pargs.free_from_os_str(parse_path)?,
            title: pargs.opt_value_from_str("--title")?.unwrap_or("vu".into()),
            no_focus: pargs.contains(["-n", "--no-focus"]),
        };
        Ok(args)
    }
}
fn parse_path(s: &std::ffi::OsStr) -> Result<PathBuf, &'static str> {
    Ok(s.into())
}

fn main() -> Result<(), Error> {
    let args = match Args::parse() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: {}.", e);
            exit(1);
        }
    };

    let event_loop = EventLoopBuilder::<RequestNextFrame>::with_user_event()
        .build()
        .expect("Failed to create event loop");

    // Get the primary monitor's size, so we can scale images if needed.
    let mon = event_loop
        .available_monitors()
        .next()
        .expect("Couldn't find the primary monitor");
    let mon_size = mon.size();
    let scale_factor = mon.scale_factor();
    let (max_width, max_height) = (mon_size.width - MARGIN * 2, mon_size.height - MARGIN * 2);

    let window = Window {
        title: &args.title,
        scale_factor,
        should_focus: !args.no_focus,
    };

    let image = img::read_image(&args.path, (max_width, max_height));
    match image {
        Some(image) => {
            window.display(event_loop, image)?;
        }
        None => exit(1),
    }
    Ok(())
}

struct Animator {
    handle: Option<JoinHandle<()>>,

    /// A flag indicating when the frame delay thread
    /// should terminate.
    is_running: Arc<AtomicBool>,
}
impl Animator {
    fn new(event_loop: &EventLoop<RequestNextFrame>, delays: &[f64]) -> Self {
        let is_running = Arc::new(AtomicBool::new(true));

        // Setup a separate thread to handle frame
        // delays/advancement.
        let should_run = Arc::clone(&is_running);
        let proxy = event_loop.create_proxy();
        let delays = delays.to_vec();
        let handle = thread::spawn(move || {
            'outer: while should_run.load(Ordering::SeqCst) {
                for delay in &delays {
                    thread::sleep(Duration::from_secs_f64(*delay));
                    if proxy.send_event(RequestNextFrame).is_err() {
                        break 'outer;
                    }
                }
            }
        });

        Self {
            is_running,
            handle: Some(handle),
        }
    }
}
impl Drop for Animator {
    fn drop(&mut self) {
        self.is_running.store(false, Ordering::SeqCst);
        self.handle.take().unwrap().join().unwrap();
    }
}

struct Window<'a> {
    title: &'a str,
    should_focus: bool,
    scale_factor: f64,
}
impl Window<'_> {
    fn display(
        &self,
        event_loop: EventLoop<RequestNextFrame>,
        mut img: img::Image,
    ) -> Result<(), Error> {
        let (width, height) = img.size();
        let size = PhysicalSize::new(width as f64, height as f64);

        // Note: For Wayland positioning has to happen via the window manager.
        let window = WindowBuilder::new()
            .with_title(self.title)
            .with_resizable(false)
            .with_decorations(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_inner_size(LogicalSize::<f64>::from_physical(size, self.scale_factor))
            .build(&event_loop)
            .unwrap();

        let surface_texture = SurfaceTexture::new(width, height, &window);
        let mut pixels = PixelsBuilder::new(width, height, surface_texture)
            .clear_color(CLEAR_COLOR)
            .build()?;
        pixels.frame_mut().copy_from_slice(img.next_frame());

        let _animator = img
            .delays()
            .map(|delays| Animator::new(&event_loop, delays));

        event_loop
            .run(move |event, target| match event {
                // Go to the next frame in a sequence.
                Event::UserEvent(RequestNextFrame) => {
                    let data = img.next_frame();
                    pixels.frame_mut().copy_from_slice(data);
                    if pixels.render().is_err() {
                        target.exit();
                    }
                }
                Event::WindowEvent {
                    event: WindowEvent::RedrawRequested,
                    ..
                } => {
                    if pixels.render().is_err() {
                        target.exit();
                    }
                }
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
                    if !self.should_focus {
                        std::process::Command::new("riverctl")
                            .args(["focus-view", "previous"])
                            .output()
                            .unwrap();
                    }
                }

                // Exit on Esc or Q
                Event::WindowEvent {
                    event:
                        WindowEvent::KeyboardInput {
                            event:
                                KeyEvent {
                                    physical_key: PhysicalKey::Code(key),
                                    ..
                                },
                            ..
                        },
                    ..
                } => match key {
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
            })
            .expect("Event loop failed");

        Ok(())
    }
}
