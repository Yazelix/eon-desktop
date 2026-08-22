#![forbid(unsafe_code)]

#[cfg(not(target_os = "linux"))]
compile_error!("Venus supports only Linux");

mod application;
mod launch;

use std::{env, error::Error};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn main() -> Result {
    let arguments = launch::launch_arguments(
        env::args_os().skip(1),
        env::var_os("EON_VENUS_PRESENTATION_CONTROL"),
    )?;
    application::run(arguments)
}
