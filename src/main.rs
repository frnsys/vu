use bpaf::Bpaf;
use std::path::PathBuf;

#[derive(Debug, Bpaf)]
#[bpaf(options, version)]
struct Args {
    /// Window title
    #[bpaf(short, long, fallback("vu".to_string()))]
    title: String,

    /// Window size limit
    #[bpaf(short, long)]
    max_side: Option<u32>,

    #[bpaf(positional("PATH"))]
    path: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = args().run();
    vu::run(&opts.title, &opts.path, opts.max_side)
}
