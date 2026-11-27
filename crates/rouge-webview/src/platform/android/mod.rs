#[allow(deprecated)]
mod java_page;

use super::SystemEngine;
use crate::content::WebContent;
use crate::engine::{
    EnginePage, Layout, PageCommand, Placement, WebEngine, WebViewEvent, WebViewEvents,
    WebViewOptions,
};
use crate::error::WebViewError;
use android_activity::AndroidApp;
use java_page::{JavaPage, JavaPageAPI, JavaPageNativeInterface};
use jni::errors::Error as JniError;
use jni::objects::{JClass, JObject, JString};
use jni::refs::{Global, LoaderContext};
use jni::sys::{jfloat, jint, jlong, jobject};
use jni::{Env, JavaVM};

type JniResult<T> = Result<T, JniError>;

const RADII: usize = 4;

impl JavaPageNativeInterface for JavaPageAPI {
    type Error = JniError;

    fn on_loading<'local>(
        _env: &mut Env<'local>,
        _class: JClass<'local>,
        peer: jlong,
        loading: bool,
    ) -> JniResult<()> {
        hear(peer, WebViewEvent::Loading(loading));
        Ok(())
    }

    fn on_progress<'local>(
        _env: &mut Env<'local>,
        _class: JClass<'local>,
        peer: jlong,
        progress: jfloat,
    ) -> JniResult<()> {
        hear(peer, WebViewEvent::Progress(progress));
        Ok(())
    }

    fn on_url<'local>(
        env: &mut Env<'local>,
        _class: JClass<'local>,
        peer: jlong,
        url: JString<'local>,
    ) -> JniResult<()> {
        hear(peer, WebViewEvent::Url(url.try_to_string(env)?));
        Ok(())
    }

    fn on_title<'local>(
        env: &mut Env<'local>,
        _class: JClass<'local>,
        peer: jlong,
        title: JString<'local>,
    ) -> JniResult<()> {
        hear(peer, WebViewEvent::Title(title.try_to_string(env)?));
        Ok(())
    }

    fn on_history<'local>(
        _env: &mut Env<'local>,
        _class: JClass<'local>,
        peer: jlong,
        back: bool,
        forward: bool,
    ) -> JniResult<()> {
        hear(peer, WebViewEvent::History { back, forward });
        Ok(())
    }

    fn on_failed<'local>(
        env: &mut Env<'local>,
        _class: JClass<'local>,
        peer: jlong,
        reason: JString<'local>,
    ) -> JniResult<()> {
        hear(peer, WebViewEvent::Failed(reason.try_to_string(env)?));
        Ok(())
    }

    fn on_message<'local>(
        env: &mut Env<'local>,
        _class: JClass<'local>,
        peer: jlong,
        message: JString<'local>,
    ) -> JniResult<()> {
        hear(peer, WebViewEvent::Message(message.try_to_string(env)?));
        Ok(())
    }

    fn on_closed<'local>(
        _env: &mut Env<'local>,
        _class: JClass<'local>,
        peer: jlong,
    ) -> JniResult<()> {
        // SAFETY: the Java page hands back its peer once, after its last event.
        drop(unsafe { Box::from_raw(peer as *mut WebViewEvents) });
        Ok(())
    }
}

fn hear(peer: jlong, event: WebViewEvent) {
    // SAFETY: the Java page reports events on the UI thread only until it is closed, and frees
    // the peer through `on_closed` on that thread only after closing.
    let listener = unsafe { &*(peer as *const WebViewEvents) };
    listener.send(event);
}

impl WebEngine for SystemEngine<AndroidApp> {
    fn open(
        &self,
        options: WebViewOptions,
        events: WebViewEvents,
    ) -> Result<Box<dyn EnginePage>, WebViewError> {
        let listener = Box::into_raw(Box::new(events));
        let opened = java(&self.0, |env, activity| {
            JavaPageAPI::get(env, &LoaderContext::FromObject(activity))?;
            let page = JavaPage::new(env, activity, listener as jlong, options.inspectable)?;
            env.new_global_ref(page)
        });
        match opened {
            Ok(page) => Ok(Box::new(Page {
                app: self.0.clone(),
                java: Some(page),
            })),
            Err(error) => {
                // SAFETY: no Java page was made to receive this peer, so nothing else frees it.
                drop(unsafe { Box::from_raw(listener) });
                Err(WebViewError::Create(error.to_string()))
            }
        }
    }
}

fn java<T>(
    app: &AndroidApp,
    call: impl FnOnce(&mut Env<'_>, &JObject<'_>) -> JniResult<T>,
) -> JniResult<T> {
    let raw: jobject = app.activity_as_ptr().cast();
    // SAFETY: `android-activity` stores the live Java VM in `AndroidApp`.
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) };
    vm.attach_current_thread(|env| {
        // SAFETY: the context is the activity's global reference, kept alive by `app`.
        let activity = unsafe { env.as_cast_raw::<Global<JObject<'_>>>(&raw)? };
        call(env, &activity)
    })
}

struct Page {
    app: AndroidApp,
    java: Option<Global<JavaPage<'static>>>,
}

impl Page {
    fn call(&self, call: impl FnOnce(&mut Env<'_>, &JavaPage<'_>) -> JniResult<()>) {
        if let Some(page) = &self.java {
            let _ = java(&self.app, |env, _| call(env, page));
        }
    }

    fn place(&self, placement: &Placement) {
        let interactive = placement.interactive;
        let Some(Layout {
            clip,
            page,
            unzoomed,
            corners,
            zoom,
            ..
        }) = placement.layout(placement.device_scale)
        else {
            return self.call(|env, java| java.hide(env, interactive));
        };
        let frames = [
            clip.to_array(),
            [
                page.origin.x,
                page.origin.y,
                unzoomed.width,
                unzoomed.height,
            ],
        ]
        .as_flattened()
        .iter()
        .map(|length| length.round() as jint)
        .collect::<Vec<_>>();
        self.call(|env, java| {
            let placed = env.new_int_array(frames.len())?;
            placed.set_region(env, 0, &frames)?;
            let radii = env.new_float_array(RADII)?;
            radii.set_region(env, 0, &corners.to_array())?;
            java.place(env, interactive, &placed, zoom, &radii)
        });
    }
}

impl EnginePage for Page {
    fn apply(&mut self, command: PageCommand) {
        match command {
            PageCommand::Load(WebContent::Url(url)) => self.call(|env, java| {
                let url = env.new_string(url)?;
                java.load_url(env, &url)
            }),
            PageCommand::Load(WebContent::Html { html, base_url }) => self.call(|env, java| {
                let html = env.new_string(html)?;
                let base_url = match base_url {
                    Some(base_url) => env.new_string(base_url)?,
                    None => JString::null(),
                };
                java.load_html(env, &html, &base_url)
            }),
            PageCommand::Place(placement) => self.place(&placement),
            PageCommand::Reload => self.call(|env, java| java.reload(env)),
            PageCommand::Stop => self.call(|env, java| java.stop(env)),
            PageCommand::Back => self.call(|env, java| java.go_back(env)),
            PageCommand::Forward => self.call(|env, java| java.go_forward(env)),
            PageCommand::Post(message) => self.call(|env, java| {
                let message = env.new_string(message)?;
                java.post(env, &message)
            }),
        }
    }
}

impl Drop for Page {
    fn drop(&mut self) {
        self.call(|env, java| java.close(env));
        let page = self.java.take();
        let _ = java(&self.app, |_, _| {
            drop(page);
            Ok(())
        });
    }
}
