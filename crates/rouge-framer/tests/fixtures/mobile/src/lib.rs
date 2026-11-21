use rouge::prelude::*;

pub fn app() -> App {
    App::new().window(WindowOptions::default().title("Rouge Mobile Smoke"), || {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(Color::rgb(0x331c4d))
            .child(
                text("Rouge Mobile Smoke")
                    .text_size(24.0)
                    .color(Color::rgb(0xffffff)),
            )
    })
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
extern "Rust" fn android_main(android: rouge::platform::AndroidApp) {
    if let Err(error) = app().android(android).run() {
        eprintln!("rouge: {error}");
    }
}
