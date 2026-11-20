use crate::{RougePublish, Target};
use framer::github::{Job, Step, Triggers, Workflow};
use framer::yaml::Yaml;
use std::fmt::Write;

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
    Step::uses(ARTIFACT)
        .name("Save package for inspection")
        .condition(CHECKING)
        .with("name", format!("rouge-{target}-check"))
        .with("path", format!(".rouge/out/{target}/{path}"))
        .with("if-no-files-found", "error")
}

pub(crate) fn secrets(step: Step, names: &[&str]) -> Step {
    names.iter().fold(step, |step, name| {
        step.env(*name, format!("${{{{ secrets.{name} }}}}"))
    })
}

pub(crate) fn render(config: &RougePublish) -> String {
    let mut groups = vec!["all".to_owned()];
    let mut workflow = Workflow::new("rouge-publish", "Publish Rouge app")
        .on(Triggers::new().workflow_dispatch())
        .concurrency("rouge-app-publish", false)
        .env("ROUGE_CHANNEL", "${{ inputs.channel }}")
        .env(
            "ROUGE_BUILD_NUMBER",
            "${{ inputs.build_number || github.run_number }}",
        )
        .permission("contents", "read");
    for platform in config.platforms() {
        workflow = workflow.job(platform.job(config));
        if !groups.contains(&platform.group()) {
            groups.push(platform.group());
        }
    }
    let mut document = workflow.to_yaml();
    if let Some(dispatch) = child(&mut document, "on").and_then(|on| child(on, "workflow_dispatch"))
    {
        *dispatch = Yaml::map().with("inputs", inputs(groups));
    }
    for platform in config.platforms() {
        let run_if = format!(
            "${{{{ github.ref == format('refs/heads/{{0}}', github.event.repository.default_branch) && (inputs.target == 'all' || inputs.target == '{}') }}}}",
            platform.group()
        );
        let job = child(&mut document, "jobs")
            .and_then(|jobs| child(jobs, &platform.target().to_string()));
        if let Some(Yaml::Map(entries)) = job {
            entries.insert(0, ("if".into(), run_if.into()));
            entries.insert(
                entries.len() - 1,
                ("environment".into(), platform.environment()),
            );
        }
    }
    document.to_yaml_string()
}

pub(crate) fn checklist(config: &RougePublish) -> String {
    let mut doc = String::from(include_str!("checklist.md"));
    for platform in config.platforms() {
        let _ = writeln!(
            doc,
            "\n{}\nLocal build: `cargo rouge build {}`.{}",
            platform.guide(),
            platform.target(),
            platform.requirements()
        );
    }
    doc
}

fn inputs(groups: Vec<String>) -> Yaml {
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
        .with("target", choice("Target to run", groups.into(), "all"))
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
