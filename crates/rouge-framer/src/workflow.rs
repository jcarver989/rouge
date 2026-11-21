use crate::{RougePublish, Target};
use framer::github::{Job, Step, Triggers, Workflow};
use framer::yaml::Yaml;

pub(crate) const PREVIEWING: &str = "${{ inputs.channel == 'preview' }}";
pub(crate) const CHECKING: &str = "${{ inputs.channel == 'check' }}";
pub(crate) const PUBLISHING: &str = "${{ inputs.channel != 'check' }}";
pub(crate) const CHECKOUT: &str = "actions/checkout@d23441a48e516b6c34aea4fa41551a30e30af803";
const ARTIFACT: &str = "actions/upload-artifact@b7c566a772e6b6bfb58ed0dc250532a479d7789f";

#[derive(Clone, Debug, Default)]
pub(crate) struct Ci {
    pub(crate) runner: Option<String>,
    pub(crate) minutes: Option<u32>,
    pub(crate) setup: Vec<Step>,
}

impl Ci {
    pub(crate) fn job(&self, target: Target, runner: &str, minutes: u32, checkout: Step) -> Job {
        let job = Job::new(target.to_string())
            .runs_on(self.runner.as_deref().unwrap_or(runner))
            .timeout_minutes(self.minutes.unwrap_or(minutes))
            .step(checkout);
        self.setup.iter().cloned().fold(job, Job::step)
    }
}

macro_rules! ci_options {
    ($platform:ty) => {
        impl $platform {
            pub fn runner(mut self, image: impl Into<String>) -> Self {
                self.ci.runner = Some(image.into());
                self
            }

            pub fn timeout_minutes(mut self, minutes: u32) -> Self {
                self.ci.minutes = Some(minutes);
                self
            }

            pub fn setup(mut self, step: framer::github::Step) -> Self {
                self.ci.setup.push(step);
                self
            }
        }
    };
}
pub(crate) use ci_options;

pub(crate) fn checkout() -> Step {
    Step::uses(CHECKOUT).with("persist-credentials", "false")
}

pub(crate) fn rouge(name: &str, action: &str, target: Target) -> Step {
    Step::run(format!("cargo rouge {action} {target}")).name(name)
}

pub(crate) fn inspect(target: Target, path: &str) -> Step {
    artifact(
        target,
        "Save package for inspection",
        "check",
        path,
        CHECKING,
    )
}

pub(crate) fn artifact(
    target: Target,
    title: &str,
    name: &str,
    path: &str,
    condition: &str,
) -> Step {
    let step = Step::uses(ARTIFACT)
        .name(title)
        .condition(condition)
        .with("name", format!("rouge-{target}-{name}"))
        .with("path", format!(".rouge/out/{target}/{path}"))
        .with("if-no-files-found", "error");
    if target.mobile() {
        step.with("overwrite", "true")
            .with("include-hidden-files", "true")
    } else {
        step
    }
}

pub(crate) fn secrets(step: Step, names: &[&str]) -> Step {
    names.iter().fold(step, |step, name| {
        step.env(*name, format!("${{{{ secrets.{name} }}}}"))
    })
}

pub(crate) fn preview_secrets(step: Step, names: &[&str]) -> Step {
    names.iter().fold(step, |step, name| {
        step.env(
            *name,
            format!("${{{{ inputs.channel == 'preview' && secrets.{name} || '' }}}}"),
        )
    })
}

pub(crate) fn render(config: &RougePublish) -> String {
    let mut targets = vec!["all".to_owned()];
    let mut workflow = Workflow::new("rouge-publish", "Publish Rouge app")
        .on(Triggers::new().workflow_dispatch())
        .concurrency("rouge-app-publish", false)
        .env("ROUGE_CHANNEL", "${{ inputs.channel }}")
        .env(
            "ROUGE_BUILD_NUMBER",
            "${{ inputs.build_number || github.run_number }}",
        )
        .permission("contents", "read");
    let mut jobs = Vec::new();
    for platform in config.platforms() {
        let target = platform.target();
        let name = target.to_string();
        let branch = format!(
            "github.ref == format('refs/heads/{{0}}', github.event.repository.default_branch) && (inputs.target == 'all' || inputs.target == '{name}')"
        );
        let condition = |extra: &str| format!("${{{{ {branch}{extra} }}}}");
        let channels = if target.mobile() {
            " && inputs.channel != 'production'"
        } else {
            ""
        };
        workflow = workflow.job(platform.job(config));
        jobs.push((name.clone(), condition(channels), platform.environment()));
        if let Some(preview) = platform.preview_job(config) {
            jobs.push((
                preview.id().to_owned(),
                condition(" && inputs.channel == 'preview'"),
                format!("rouge-{name}-preview").into(),
            ));
            workflow = workflow.job(preview);
        }
        targets.push(name);
    }
    let mut document = workflow.to_yaml();
    if let Some(dispatch) = child(&mut document, "on").and_then(|on| child(on, "workflow_dispatch"))
    {
        *dispatch = Yaml::map().with("inputs", inputs(targets));
    }
    for (id, run_if, environment) in jobs {
        if let Some(Yaml::Map(entries)) =
            child(&mut document, "jobs").and_then(|jobs| child(jobs, &id))
        {
            entries.insert(0, ("if".into(), run_if.into()));
            entries.insert(entries.len() - 1, ("environment".into(), environment));
        }
    }
    document.to_yaml_string()
}

fn inputs(targets: Vec<String>) -> Yaml {
    let choice = |description: &str, options: Yaml, default: &str| {
        Yaml::map()
            .with("description", description)
            .with("type", "choice")
            .with("options", options)
            .with("default", default)
    };
    Yaml::map()
        .with(
            "channel",
            choice(
                "Check packages, publish a preview, or release to production",
                vec!["check", "preview", "production"].into(),
                "check",
            ),
        )
        .with("target", choice("Target to run", targets.into(), "all"))
        .with(
            "build_number",
            Yaml::map()
                .with(
                    "description",
                    "Optional store build number override (defaults to GitHub run number)",
                )
                .with("type", "string"),
        )
}

fn child<'a>(yaml: &'a mut Yaml, key: &str) -> Option<&'a mut Yaml> {
    let Yaml::Map(entries) = yaml else {
        return None;
    };
    entries
        .iter_mut()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value)
}
