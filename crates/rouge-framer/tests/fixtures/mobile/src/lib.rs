use rouge::prelude::*;
use rouge::widgets::{field, sheet};

pub fn app() -> App {
    App::new().window(WindowOptions::default().title("Rouge Mobile Smoke"), form)
}

fn form() -> impl View {
    let (name, notes) = (signal(String::new()), signal(String::new()));
    let (saved, details) = (signal(String::new()), signal(false));
    let rows = signal((1..=40).collect::<Vec<u32>>());
    let email = InputHints {
        content: ContentType::Email,
        enter: EnterKey::Next,
        autocorrect: false,
    };
    let save = move || saved.set(format!("Saved {} · {}", name.get(), notes.get()));
    let content = scroll()
        .size_full()
        .flex()
        .col()
        .gap(16.0)
        .p(16.0)
        .child(text("Rouge Mobile Smoke").text_size(24.0))
        .child(field(
            "Email".into(),
            text_input(name)
                .placeholder("name@example.com")
                .input_hints(email)
                .test_id("email"),
        ))
        .child(field(
            "Notes".into(),
            text_input(notes)
                .multiline()
                .min_rows(3)
                .placeholder("Anything else")
                .test_id("notes"),
        ))
        .child(
            div()
                .flex()
                .gap(12.0)
                .child(button("Save").on_click(save).test_id("save"))
                .child(
                    button("Details")
                        .variant(ButtonVariant::Secondary)
                        .on_click(move || details.set(true))
                        .test_id("details"),
                ),
        )
        .child(text(saved).test_id("saved"))
        .child(each(
            rows,
            |row| *row,
            |row| text(move || format!("Row {}", row.get())).py(12.0),
        ));
    div()
        .size_full()
        .child(scroll_view(content).size_full())
        .child(sheet(details, move || {
            div()
                .flex()
                .col()
                .gap(12.0)
                .p(24.0)
                .child(text("Details").text_size(20.0))
                .child(button("Close").on_click(move || details.set(false)))
        }))
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
extern "Rust" fn android_main(android: rouge::platform::AndroidApp) {
    if let Err(error) = app().android(android).run() {
        eprintln!("rouge: {error}");
    }
}
