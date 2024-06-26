use std::{
    fs::File,
    io::BufReader,
    path::PathBuf,
    process::exit,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use image::{
    codecs::gif::GifDecoder, imageops::FilterType, AnimationDecoder, GenericImageView, ImageDecoder,
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

/// We can have either a single image
/// or a sequence of images (i.e. an animated gif).
enum Image<'a> {
    Single(&'a [u8]),
    Sequence(Vec<&'a [u8]>),
}
impl Image<'_> {
    /// Get a frame by index.
    /// This automatically wraps the index around.
    fn get_frame(&self, idx: usize) -> &[u8] {
        match self {
            Self::Single(data) => data,
            Self::Sequence(frames) => frames[idx % frames.len()],
        }
    }
}

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

    // Use this as a flag to indicate when the frame delay thread
    // should terminate.
    let is_running = Arc::new(AtomicBool::new(true));

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
    let (mon_width, mon_height) = (mon_size.width - MARGIN * 2, mon_size.height - MARGIN * 2);

    // Handle gifs.
    if args.path.extension().is_some_and(|ext| ext == "gif") {
        // Load and extract all the frames.
        let file = File::open(args.path).expect("Failed to read gif");
        let reader = BufReader::new(file);
        let decoder = GifDecoder::new(reader).expect("Failed to decode gif");
        let size = decoder.dimensions();
        let decoded = decoder
            .into_frames()
            .collect_frames()
            .expect("Failed to decode frames");
        let (frames, delays): (Vec<_>, Vec<_>) = decoded
            .iter()
            .map(|f| {
                let (num, den) = f.delay().numer_denom_ms();
                let delay = (num as f64 / den as f64) / 1000.;
                (f.buffer().as_raw().as_slice(), delay)
            })
            .unzip();

        // Setup a separate thread to handle frame
        // delays/advancement.
        let should_run = Arc::clone(&is_running);
        let proxy = event_loop.create_proxy();
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

        run(
            event_loop,
            &args.title,
            Image::Sequence(frames),
            !args.no_focus,
            size,
            scale_factor,
        )?;

        // We're quitting, let the thread know
        // and wait for it to finish.
        is_running.store(false, Ordering::SeqCst);
        handle.join().unwrap();

    // Display a single static image.
    } else {
        match image::open(args.path) {
            Ok(mut img) => {
                // Resize to fit if needed.
                let size = img.dimensions();
                if size.0 > mon_width || size.1 > mon_height {
                    img = img.resize(mon_width, mon_height, FilterType::Triangle);
                }
                let rgba = img.to_rgba8();
                let pixels: &[u8] = rgba.as_raw();
                run(
                    event_loop,
                    &args.title,
                    Image::Single(pixels),
                    !args.no_focus,
                    img.dimensions(),
                    scale_factor,
                )?;
            }
            Err(e) => {
                eprintln!("Failed to load image: {}", e);
                exit(1);
            }
        }
    }

    Ok(())
}

fn run(
    event_loop: EventLoop<RequestNextFrame>,
    title: &str,
    img: Image,
    focus: bool,
    (width, height): (u32, u32),
    scale_factor: f64,
) -> Result<(), Error> {
    let size = PhysicalSize::new(width as f64, height as f64);

    // Note: For Wayland positioning has to happen via the window manager.
    let window = WindowBuilder::new()
        .with_title(title)
        .with_resizable(false)
        .with_decorations(false)
        .with_window_level(WindowLevel::AlwaysOnTop)
        .with_inner_size(LogicalSize::<f64>::from_physical(size, scale_factor))
        .build(&event_loop)
        .unwrap();

    let surface_texture = SurfaceTexture::new(width, height, &window);
    let mut pixels = PixelsBuilder::new(width, height, surface_texture)
        .clear_color(CLEAR_COLOR)
        .build()?;
    pixels.frame_mut().copy_from_slice(img.get_frame(0));

    let mut frame_idx = 0;
    event_loop
        .run(move |event, target| match event {
            // Go to the next frame in a sequence.
            Event::UserEvent(RequestNextFrame) => {
                frame_idx += 1;
                let data = img.get_frame(frame_idx);
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
                if !focus {
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
