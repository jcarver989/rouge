#[path = "shared/web_thread_view.rs"]
mod web_thread_view;

use rouge::{App, AppError, WindowOptions};

fn main() -> Result<(), AppError> {
    let options = WindowOptions::default()
        .title("Rouge • Web views")
        .size(880.0, 760.0);
    App::new()
        .window(options, web_thread_view::web_thread)
        .run()
}
