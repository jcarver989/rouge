#![cfg(target_os = "android")]
#![allow(unsafe_code)]

#[path = "shared/todo/mod.rs"]
mod todo;

use rouge::platform::AndroidApp;
use rouge::prelude::*;

#[unsafe(no_mangle)]
extern "Rust" fn android_main(android: AndroidApp) {
    let options = WindowOptions::default().title("Rouge • TodoMVC");
    let run = App::new()
        .android(android)
        .window(options, || {
            provide_storage(app_storage("Rouge TodoMVC"));
            todo::todo_app()
        })
        .run();
    if let Err(error) = run {
        eprintln!("rouge: {error}");
    }
}
