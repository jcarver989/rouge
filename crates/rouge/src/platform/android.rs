pub use android_activity::AndroidApp;

mod host;

use host::Exit;

use super::Platform;
use crate::app::{App, AppError};
use android_activity::ndk::configuration::UiModeNight;
use jni::errors::Error as JniError;
use jni::objects::{JObject, JString, JValue};
use jni::refs::Global;
use jni::sys::jobject;
use jni::{Env, JavaVM, jni_sig, jni_str};
use rouge_core::{Appearance, Clipboard, PasteRequest};
use rouge_render::Edges;
#[cfg(feature = "open-url")]
use std::io::Error as IoError;
#[cfg(feature = "persist")]
use std::path::PathBuf;
use std::process::exit;
use std::sync::OnceLock;
use thiserror::Error;

type JniResult<T> = Result<T, JniError>;

static APP: OnceLock<AndroidApp> = OnceLock::new();

#[derive(Debug, Error)]
pub enum Error {
    #[error("no Android activity: pass the AndroidApp from android_main to App::android")]
    NoActivity,
    #[error("Android shows one window per activity, but the app opens {0}")]
    MultipleWindows(usize),
}

impl App {
    pub fn android(mut self, android: AndroidApp) -> Self {
        self.platform = Some(android);
        self
    }
}

pub(crate) struct Android;

impl Platform for Android {
    type Options = Option<AndroidApp>;

    const MOBILE: bool = true;
    const MENU_BAR: bool = false;

    fn run(app: App, options: Self::Options) -> Result<(), AppError> {
        if app.windows.len() > 1 {
            return Err(Error::MultipleWindows(app.windows.len()).into());
        }
        let android = options.ok_or(Error::NoActivity)?;
        let _ = APP.set(android.clone());
        match host::run(app, android)? {
            Exit::Destroyed => terminate(),
            Exit::Quit => finish(),
        }
    }

    #[cfg(feature = "persist")]
    fn default_config_dir(name: &str) -> Option<PathBuf> {
        let dir = APP.get()?.internal_data_path()?;
        Some(dir.join(name.to_lowercase()))
    }

    #[cfg(feature = "open-url")]
    fn open_url(url: &str) -> Result<(), IoError> {
        java(|env, activity| {
            let url = env.new_string(url)?;
            let uri = env
                .call_static_method(
                    jni_str!("android/net/Uri"),
                    jni_str!("parse"),
                    jni_sig!("(Ljava/lang/String;)Landroid/net/Uri;"),
                    &[JValue::from(&url)],
                )?
                .l()?;
            let action = env.new_string("android.intent.action.VIEW")?;
            let intent = env.new_object(
                jni_str!("android/content/Intent"),
                jni_sig!("(Ljava/lang/String;Landroid/net/Uri;)V"),
                &[JValue::from(&action), JValue::from(&uri)],
            )?;
            env.call_method(
                activity,
                jni_str!("startActivity"),
                jni_sig!("(Landroid/content/Intent;)V"),
                &[JValue::from(&intent)],
            )?
            .v()
        })
        .map_err(IoError::other)
    }
}

fn appearance() -> Appearance {
    let night = APP.get().map(|app| app.config().ui_mode_night());
    if night == Some(UiModeNight::Yes) {
        Appearance::Dark
    } else {
        Appearance::Light
    }
}

fn refresh_rate() -> Option<f32> {
    java(|env, activity| {
        let manager = env
            .call_method(
                activity,
                jni_str!("getWindowManager"),
                jni_sig!("()Landroid/view/WindowManager;"),
                &[],
            )?
            .l()?;
        let display = env
            .call_method(
                &manager,
                jni_str!("getDefaultDisplay"),
                jni_sig!("()Landroid/view/Display;"),
                &[],
            )?
            .l()?;
        env.call_method(&display, jni_str!("getRefreshRate"), jni_sig!("()F"), &[])?
            .f()
    })
    .ok()
}

fn leave() {
    let _ = java(|env, activity| {
        env.call_method(
            activity,
            jni_str!("moveTaskToBack"),
            jni_sig!("(Z)Z"),
            &[JValue::from(true)],
        )?
        .z()
    });
}

fn finish() -> ! {
    let _ = java(|env, activity| {
        env.call_method(
            activity,
            jni_str!("finishAndRemoveTask"),
            jni_sig!("()V"),
            &[],
        )?
        .v()
    });
    terminate()
}

fn terminate() -> ! {
    let _ = java(|env, _| {
        let process = jni_str!("android/os/Process");
        let id = env
            .call_static_method(process, jni_str!("myPid"), jni_sig!("()I"), &[])?
            .i()?;
        env.call_static_method(
            process,
            jni_str!("killProcess"),
            jni_sig!("(I)V"),
            &[JValue::from(id)],
        )?
        .v()
    });
    exit(0)
}

struct AndroidClipboard;

impl Clipboard for AndroidClipboard {
    fn write(&mut self, text: &str) {
        let _ = java(|env, activity| {
            let manager = clipboard_manager(env, activity)?;
            let (label, text) = (env.new_string("")?, env.new_string(text)?);
            let clip = env
                .call_static_method(
                    jni_str!("android/content/ClipData"),
                    jni_str!("newPlainText"),
                    jni_sig!(
                        "(Ljava/lang/CharSequence;Ljava/lang/CharSequence;)Landroid/content/ClipData;"
                    ),
                    &[JValue::from(&label), JValue::from(&text)],
                )?
                .l()?;
            env.call_method(
                &manager,
                jni_str!("setPrimaryClip"),
                jni_sig!("(Landroid/content/ClipData;)V"),
                &[JValue::from(&clip)],
            )?
            .v()
        });
    }

    fn read(&mut self, request: PasteRequest) {
        let text = java(|env, activity| {
            let manager = clipboard_manager(env, activity)?;
            let clip = env
                .call_method(
                    &manager,
                    jni_str!("getPrimaryClip"),
                    jni_sig!("()Landroid/content/ClipData;"),
                    &[],
                )?
                .l()?;
            if clip.is_null() {
                return Ok(None);
            }
            let item = env
                .call_method(
                    &clip,
                    jni_str!("getItemAt"),
                    jni_sig!("(I)Landroid/content/ClipData$Item;"),
                    &[JValue::from(0)],
                )?
                .l()?;
            let text = env
                .call_method(
                    &item,
                    jni_str!("coerceToText"),
                    jni_sig!("(Landroid/content/Context;)Ljava/lang/CharSequence;"),
                    &[JValue::from(activity)],
                )?
                .l()?;
            let text = env
                .call_method(
                    &text,
                    jni_str!("toString"),
                    jni_sig!("()Ljava/lang/String;"),
                    &[],
                )?
                .l()?;
            let text = env.cast_local::<JString>(text)?;
            text.try_to_string(env).map(Some)
        });
        if let Ok(Some(text)) = text {
            request.text(text);
        }
    }
}

fn clipboard_manager<'a>(env: &mut Env<'a>, activity: &JObject<'_>) -> JniResult<JObject<'a>> {
    let service = env.new_string("clipboard")?;
    env.call_method(
        activity,
        jni_str!("getSystemService"),
        jni_sig!("(Ljava/lang/String;)Ljava/lang/Object;"),
        &[JValue::from(&service)],
    )?
    .l()
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Insets {
    safe: Edges,
    keyboard: f32,
}

fn insets() -> Option<Insets> {
    java(|env, activity| {
        let window = env
            .call_method(
                activity,
                jni_str!("getWindow"),
                jni_sig!("()Landroid/view/Window;"),
                &[],
            )?
            .l()?;
        let view = env
            .call_method(
                &window,
                jni_str!("getDecorView"),
                jni_sig!("()Landroid/view/View;"),
                &[],
            )?
            .l()?;
        let root = env
            .call_method(
                &view,
                jni_str!("getRootWindowInsets"),
                jni_sig!("()Landroid/view/WindowInsets;"),
                &[],
            )?
            .l()?;
        if root.is_null() {
            return Ok(None);
        }
        let mut mask = |name| {
            env.call_static_method(
                jni_str!("android/view/WindowInsets$Type"),
                name,
                jni_sig!("()I"),
                &[],
            )?
            .i()
        };
        let safe = mask(jni_str!("systemBars"))? | mask(jni_str!("displayCutout"))?;
        let keyboard = mask(jni_str!("ime"))?;
        let mut edges = |mask: i32| -> JniResult<Edges> {
            let insets = env
                .call_method(
                    &root,
                    jni_str!("getInsets"),
                    jni_sig!("(I)Landroid/graphics/Insets;"),
                    &[JValue::from(mask)],
                )?
                .l()?;
            let mut side = |name| -> JniResult<f32> {
                Ok(env.get_field(&insets, name, jni_sig!("I"))?.i()? as f32)
            };
            Ok(Edges {
                top: side(jni_str!("top"))?,
                right: side(jni_str!("right"))?,
                bottom: side(jni_str!("bottom"))?,
                left: side(jni_str!("left"))?,
            })
        };
        Ok(Some(Insets {
            safe: edges(safe)?,
            keyboard: edges(keyboard)?.bottom,
        }))
    })
    .ok()
    .flatten()
}

#[allow(unsafe_code)]
fn java<T>(call: impl FnOnce(&mut Env<'_>, &JObject<'_>) -> JniResult<T>) -> JniResult<T> {
    let app = APP.get().ok_or(JniError::UninitializedJavaVM)?;
    let raw: jobject = app.activity_as_ptr().cast();
    JavaVM::singleton()?.attach_current_thread(|env| {
        let activity = unsafe { env.as_cast_raw::<Global<JObject<'_>>>(&raw)? };
        call(env, &activity)
    })
}
