#![cfg(unix)]

use rouge_framer::{Action, Channel, GitHubPages, Packager, RougePublish, Target};
use std::fs;
use std::io::Error;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::tempdir;

#[test]
fn preserves_the_other_pages_channel_from_a_fresh_shallow_checkout()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let remote = temp.path().join("remote.git");
    let url = format!("file://{}", remote.display());
    git(temp.path(), &["init", "--bare", "remote.git"])?;
    git(temp.path(), &["clone", &url, "source"])?;
    let source = temp.path().join("source");
    git(&source, &["checkout", "-b", "main"])?;
    git(
        &source,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "--allow-empty",
            "-m",
            "App",
        ],
    )?;
    git(&source, &["push", "origin", "main"])?;
    for (checkout, channel, content) in [
        ("first", Channel::Preview, "preview v1"),
        ("second", Channel::Production, "production v1"),
        ("failed", Channel::Production, "not deployed"),
        ("third", Channel::Preview, "preview v2"),
    ] {
        git(
            temp.path(),
            &["clone", "--depth=1", "--branch=main", &url, checkout],
        )?;
        let repo = temp.path().join(checkout);
        fs::create_dir_all(repo.join(".rouge/out/web"))?;
        fs::write(repo.join(".rouge/out/web/index.html"), content)?;
        fs::write(repo.join("Cargo.toml"), MANIFEST)?;
        fs::write(repo.join("Cargo.lock"), LOCK)?;
        fs::create_dir_all(repo.join("src"))?;
        fs::write(repo.join("src/main.rs"), "fn main() {}\n")?;
        let packager = Packager::new(RougePublish::new("com.example.app").web(GitHubPages), &repo)
            .channel(channel);
        packager.run(Action::Stage, Target::Web)?;
        if checkout == "failed" {
            assert_eq!(
                fs::read_to_string(repo.join(".rouge/out/pages/index.html"))?,
                content
            );
            continue;
        }
        packager.run(Action::Publish, Target::Web)?;
        let site = repo.join(".rouge/out/pages");
        if checkout == "first" {
            assert!(fs::read_to_string(site.join("index.html"))?.contains("./preview/"));
        } else {
            assert_eq!(
                fs::read_to_string(site.join("index.html"))?,
                "production v1"
            );
        }
        assert_eq!(
            fs::read_to_string(site.join("preview/index.html"))?,
            if checkout == "third" {
                "preview v2"
            } else {
                "preview v1"
            }
        );
        assert!(!site.join(".git").exists());
    }
    Ok(())
}

const MANIFEST: &str =
    "[workspace]\n\n[package]\nname = \"app\"\nversion = \"1.0.0\"\nedition = \"2024\"\n";
const LOCK: &str = "version = 4\n\n[[package]]\nname = \"app\"\nversion = \"1.0.0\"\n";

fn git(root: &Path, args: &[&str]) -> Result<(), Error> {
    success(Command::new("git").current_dir(root).args(args).output()?)
}

fn success(output: Output) -> Result<(), Error> {
    if !output.status.success() {
        return Err(Error::other(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    Ok(())
}
