pub struct SystemEngine<T>(pub T);

cfg_select! {
    any(target_os = "macos", target_os = "ios") => {
        mod apple;
        mod shared;
    }
    target_os = "android" => {
        #[allow(unsafe_code)]
        mod android;
    }
    target_os = "linux" => {
        mod linux;
        mod shared;
    }
    target_arch = "wasm32" => {
        mod shared;
        mod web;
    }
    _ => {
        mod unsupported;
    }
}
