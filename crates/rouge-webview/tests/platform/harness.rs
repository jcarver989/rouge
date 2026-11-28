use std::env::args;

const VALUED_FLAGS: [&str; 6] = [
    "--format",
    "--skip",
    "--test-threads",
    "--color",
    "--logfile",
    "-Z",
];

pub(super) fn selected<'a, T>(tests: &'a [(&'static str, T)]) -> Vec<&'a (&'static str, T)> {
    let selection = Selection::parse(args().skip(1));
    if selection.list {
        if !selection.ignored {
            for (name, _) in tests {
                println!("{name}: test");
            }
        }
        return Vec::new();
    }
    if selection.ignored {
        return Vec::new();
    }
    tests
        .iter()
        .filter(|(name, _)| selection.matches(name))
        .collect()
}

#[derive(Default)]
struct Selection {
    list: bool,
    ignored: bool,
    exact: bool,
    filter: Option<String>,
}

impl Selection {
    fn parse(args: impl Iterator<Item = String>) -> Self {
        let mut selection = Self::default();
        let mut args = args;
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--list" => selection.list = true,
                "--ignored" => selection.ignored = true,
                "--exact" => selection.exact = true,
                flag if VALUED_FLAGS.contains(&flag) => {
                    args.next();
                }
                flag if flag.starts_with('-') => {}
                _ => {
                    selection.filter.get_or_insert(arg);
                }
            }
        }
        selection
    }

    fn matches(&self, name: &str) -> bool {
        match &self.filter {
            None => true,
            Some(filter) if self.exact => name == filter,
            Some(filter) => name.contains(filter.as_str()),
        }
    }
}
