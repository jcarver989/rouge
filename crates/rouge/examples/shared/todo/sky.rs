use super::tokens::NIGHT;
use rouge::core::{AnyEffect, PropFrom};
use rouge::prelude::*;
use web_time::Instant;

#[component]
pub(crate) fn night_sky() -> impl View {
    let focused = window_focused();
    let clock = stored(SkyClock::default());
    view! {
        <div absolute inset=0.0 bg=NIGHT effect={move || clock.update(|clock| clock.sky(focused.get()))}/>
    }
}

#[derive(Default)]
struct SkyClock {
    elapsed: f32,
    resumed: Option<Instant>,
}

impl SkyClock {
    fn sky(&mut self, running: bool) -> AnyEffect {
        if let Some(resumed) = self.resumed.take() {
            self.elapsed += resumed.elapsed().as_secs_f32();
        }
        if running {
            self.resumed = Some(Instant::now());
            AnyEffect::prop_from(MovingSky { from: self.elapsed })
        } else {
            AnyEffect::prop_from(StillSky { at: self.elapsed })
        }
    }
}

struct MovingSky {
    from: f32,
}

impl ShaderEffect for MovingSky {
    const WGSL: &'static str = include_str!("night.wgsl");
    const INPUTS: Inputs = Inputs::CONTENT;
    const CLOCK: Option<usize> = Some(0);

    fn uniforms(&self, uniforms: &mut [f32; 16]) {
        uniforms[1] = self.from;
    }
}

struct StillSky {
    at: f32,
}

impl ShaderEffect for StillSky {
    const WGSL: &'static str = include_str!("night.wgsl");
    const INPUTS: Inputs = Inputs::CONTENT;

    fn uniforms(&self, uniforms: &mut [f32; 16]) {
        uniforms[0] = self.at;
    }
}
