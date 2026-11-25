use rouge_core::{
    Animation, Content, Decay, Keyframes, LayerDelta, LayerId, LayerModel, LayerPose, LayerProp,
    Layers, LayoutTransition, MotionId, Nested, Point, Pose, Rect, ScrollState, Size, Spring,
    Tween,
};
use rouge_render::DrawList;

#[test]
fn sampling_at_a_time_does_not_depend_on_the_advances_before_it() {
    let direct = record(&[]);
    for seed in 1..=8 {
        let steps: Vec<_> = (1..=240)
            .map(|step| f64::from(step * seed * 7919 % 3001) / 1000.0)
            .collect();
        assert_eq!(record(&steps).samples(), direct.samples(), "seed {seed}");
    }
}

#[test]
fn recorded_commits_replay_into_a_fresh_model() {
    let recording = record(&[]);
    let mut replica = LayerModel::new();
    for (sample, delta) in &recording.frames {
        replica.apply(delta.clone());
        assert_eq!(&Sample::of(&replica, &recording.layers), sample);
    }
}

#[test]
fn a_shrinking_extent_turns_a_glide_into_a_bounce() {
    let mut layers = Layers::new();
    let layer = layers.create_layer();
    layers.set_scroll_extent(layer, Size::new(100.0, 100.0), Size::new(100.0, 1000.0));
    layers.scroll_release(layer, Point::new(0.0, 400.0));
    layers.advance(0.5);
    layers.set_scroll_extent(layer, Size::new(100.0, 100.0), Size::new(100.0, 150.0));
    assert!(layers.is_animating(), "still moving");
    layers.advance(10.0);
    assert_eq!(
        layers.scroll(layer).map(|scroll| scroll.offset.y),
        Some(50.0)
    );
    assert!(!layers.is_animating());
}

#[derive(Debug, PartialEq)]
struct Sample {
    at: f64,
    poses: Vec<Option<LayerPose>>,
    scrolls: Vec<Option<ScrollState>>,
    motions: Vec<Option<f32>>,
    layers: usize,
    settled: bool,
}

impl Sample {
    fn of(model: &LayerModel, layers: &[LayerId]) -> Self {
        let at = model.time();
        Sample {
            at,
            poses: layers.iter().map(|id| model.pose(*id)).collect(),
            scrolls: layers.iter().map(|id| model.scroll(*id)).collect(),
            motions: [1, 2].map(|id| model.motion(MotionId(id))).to_vec(),
            layers: model.layer_count(),
            settled: model.settles_at().is_none_or(|end| end <= at),
        }
    }
}

struct Recording {
    layers: Vec<LayerId>,
    frames: Vec<(Sample, LayerDelta)>,
}

impl Recording {
    fn samples(&self) -> Vec<&Sample> {
        self.frames.iter().map(|(sample, _)| sample).collect()
    }
}

type Op = fn(&mut Layers, &[LayerId]);

const SAMPLES: [f64; 10] = [0.05, 0.137, 0.2, 0.31, 0.45, 0.52, 0.8, 0.83, 1.5, 3.0];

const OPS: [(f64, Op); 4] = [
    (0.0, |c, l| {
        c.set_root(l[0]);
        c.set_frame(l[0], Rect::new(0.0, 0.0, 200.0, 200.0));
        let nested = [l[1], l[2]].map(|layer| Nested { layer, clip: None });
        c.set_content(
            l[0],
            Content::new(vec![DrawList::new(), DrawList::new()], nested.to_vec()),
        );
        c.set_transition(l[1], Some(Spring::bouncy().into()));
        c.set_layout_transition(l[1], Some(LayoutTransition::new(Spring::smooth()).size()));
        c.set_frame(l[1], Rect::new(0.0, 0.0, 50.0, 50.0));
        c.set_prop(l[1], LayerProp::TranslateX, 120.0, true);
        c.set_scroll_extent(l[2], Size::new(100.0, 100.0), Size::new(100.0, 400.0));
        c.scroll_release(l[2], Point::new(0.0, 2500.0));
        c.create_motion(MotionId(1), 0.0);
        c.bind_prop(l[2], LayerProp::Opacity, MotionId(1));
        let snappy = Spring::snappy().into();
        c.animate_motion(MotionId(1), 1.0, None, snappy, Animation::pending().0);
        c.create_motion(MotionId(2), 0.0);
        c.decay_motion(MotionId(2), 300.0, Decay::new(), Animation::pending().0);
    }),
    (0.137, |c, l| {
        c.set_prop(l[1], LayerProp::TranslateX, -40.0, true);
        c.set_frame(l[1], Rect::new(30.0, 60.0, 80.0, 40.0));
        let spin = Keyframes::new()
            .at(0.0, |p| p.rotate(0.0))
            .at(1.0, |p| p.rotate(90.0));
        c.animate(l[1], spin.duration(0.6));
        let tween = Tween::new(0.3).into();
        c.animate_motion(MotionId(1), 0.2, None, tween, Animation::pending().0);
    }),
    (0.31, |c, l| {
        c.set_scroll_extent(l[2], Size::new(100.0, 100.0), Size::new(100.0, 180.0));
        c.set_transition(l[1], Some(Tween::new(0.25).into()));
        let gone = Pose::new().opacity(0.0).scale(0.5);
        c.exit(l[1], gone, Tween::new(0.5).into());
    }),
    (0.52, |c, l| {
        c.scroll_by(l[2], Point::new(0.0, -30.0));
        c.scroll_release(l[2], Point::new(0.0, -900.0));
        c.stop_motion(MotionId(2));
    }),
];

fn record(steps: &[f64]) -> Recording {
    let mut writer = Layers::new();
    let layers: Vec<_> = (0..3).map(|_| writer.create_layer()).collect();
    let ops = OPS.iter().map(|(at, op)| (*at, Some(op), false));
    let mut times: Vec<_> = (steps.iter().map(|at| (*at, None, false)))
        .chain(ops)
        .chain(SAMPLES.iter().map(|at| (*at, None, true)))
        .collect();
    times.sort_by(|a, b| a.0.total_cmp(&b.0).then(b.1.is_some().cmp(&a.1.is_some())));
    let mut frames = Vec::new();
    for (at, op, sample) in times {
        writer.advance(at);
        if let Some(op) = op {
            op(&mut writer, &layers);
        }
        if sample {
            let delta = writer.commit().layers;
            frames.push((Sample::of(&writer, &layers), delta));
        }
    }
    Recording { layers, frames }
}
