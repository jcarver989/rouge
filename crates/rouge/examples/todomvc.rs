#[path = "shared/todo/mod.rs"]
mod todo;

use rouge::AppError;
use rouge::prelude::*;

fn main() -> Result<(), AppError> {
    let options = WindowOptions::default()
        .title("Rouge • TodoMVC")
        .size(1000.0, 760.0);
    App::new()
        .window(options, || {
            provide_storage(app_storage("Rouge TodoMVC"));
            todo::todo_app()
        })
        .run()
}
