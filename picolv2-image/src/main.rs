pub mod builder;
pub mod cli;
#[cfg(feature = "gui")]
pub mod gui;
pub mod ingen;
pub mod lv2;
pub mod turtle;

use std::{env, process::ExitCode};

fn main() -> ExitCode {
    #[cfg(feature = "gui")]
    {
        let args: Vec<String> = env::args().skip(1).collect();
        if !args.is_empty() && args[0] != "--gui" {
            return cli::run(&args);
        }
        match gui::run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("GUI error: {e}");
                ExitCode::from(1)
            }
        }
    }

    #[cfg(not(feature = "gui"))]
    {
        let args: Vec<String> = env::args().skip(1).collect();
        cli::run(&args)
    }
}

