mod application;
mod config;
mod window;

use adw::prelude::*;
use application::CamOverlayApplication;

fn main() -> glib::ExitCode {
    gstreamer::init().expect("Failed to initialize GStreamer");
    let app = CamOverlayApplication::new();
    app.run()
}
