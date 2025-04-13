use std::{path::PathBuf, process::exit};

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

fn main() -> anyhow::Result<()> {
    let args = match Args::parse() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: {}.", e);
            exit(1);
        }
    };

    vu::run(&args.title, !args.no_focus, &args.path)
}
