use framer::Project;
use rouge_framer::{Ios, PlayInternal, RougePublish, TestFlight, run};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut project = Project::new();
    project.apply(
        RougePublish::new("dev.rouge.mobile.smoke")
            .name("Rouge Mobile Smoke")
            .icon("icon.png")
            .ios(Ios::new(TestFlight::new("0")).privacy_manifest("PrivacyInfo.xcprivacy"))
            .android(PlayInternal),
    );
    run(&project)
}
