#[path = "shared/todo/mod.rs"]
mod todo;

use rouge::AppError;
use rouge::prelude::*;
use rouge::widgets::bundled_fonts;

fn main() -> Result<(), AppError> {
    App::new()
        .fonts(bundled_fonts())
        .window(
            WindowOptions::default()
                .title("Rouge • TodoMVC")
                .size(1000.0, 760.0),
            || {
                provide_storage(app_storage("Rouge TodoMVC"));
                todo::todo_app()
            },
        )
        .run()
}
