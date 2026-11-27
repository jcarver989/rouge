cfg_select! {
    any(target_os = "macos", target_os = "ios", target_os = "linux") => {
        pub(super) mod document;
        pub(super) mod inline;
        pub(super) mod message_handler;
    }
    target_arch = "wasm32" => {
        pub(super) mod document;
    }
    _ => {}
}
