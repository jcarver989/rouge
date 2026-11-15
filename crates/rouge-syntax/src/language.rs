use std::borrow::Cow;
use tree_sitter::Language as Grammar;

pub(crate) type Spec = (Grammar, &'static str, &'static str);

pub(crate) struct Language {
    pub name: &'static str,
    aliases: &'static [&'static str],
    extensions: &'static [&'static str],
    files: &'static [&'static str],
    pub spec: fn() -> Spec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct LanguageId(u8);

impl LanguageId {
    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }

    pub(crate) const fn language(self) -> &'static Language {
        &LANGUAGES[self.0 as usize]
    }

    pub(crate) const fn name(self) -> &'static str {
        self.language().name
    }

    fn find(matches: impl Fn(&Language) -> bool) -> Option<Self> {
        let index = LANGUAGES.iter().position(matches)?;
        Some(Self(u8::try_from(index).expect("language table fits u8")))
    }

    fn named(name: &str) -> Option<Self> {
        Self::find(|language| language.name == name)
    }
}

macro_rules! languages {
    ($($name:literal => $module:ident [$($alias:literal),*] $(ext [$($ext:literal),*])? $(files [$($file:literal),*])?;)*) => {
        pub(crate) const LANGUAGES: &[Language] = &[$(Language {
            name: $name,
            aliases: &[$name, $($alias),*],
            extensions: &[$($($ext),*)?],
            files: &[$($($file),*)?],
            spec: || -> Spec { ($module::language().into(), &$module::HIGHLIGHTS_QUERY, &$module::INJECTIONS_QUERY) },
        }),*];
    };
}

languages! {
    "asm" => arborium_asm ["assembly"] ext ["s"];
    "bash" => arborium_bash ["sh", "shell"] files [".bashrc"];
    "batch" => arborium_batch ["bat", "cmd"];
    "c" => arborium_c ["h"];
    "c-sharp" => arborium_c_sharp ["csharp", "c#", "cs"];
    "clojure" => arborium_clojure ["clj", "cljs", "cljc", "edn"] files ["deps.edn"];
    "cmake" => arborium_cmake [] files ["cmakelists.txt"];
    "commonlisp" => arborium_commonlisp ["common-lisp", "lisp", "cl"];
    "cpp" => arborium_cpp ["c++", "cc", "cxx", "hpp", "hh", "hxx"];
    "css" => arborium_css [];
    "dart" => arborium_dart [];
    "diff" => arborium_diff ["patch"];
    "dockerfile" => arborium_dockerfile ["docker"] files ["dockerfile", "containerfile"];
    "elixir" => arborium_elixir ["ex", "exs"];
    "erlang" => arborium_erlang ["erl", "hrl"];
    "fish" => arborium_fish [];
    "go" => arborium_go ["golang"] files ["go.mod", "go.sum"];
    "graphql" => arborium_graphql ["gql"];
    "haskell" => arborium_haskell ["hs"];
    "hcl" => arborium_hcl ["terraform", "tf", "tfvars"] files [".terraformrc"];
    "html" => arborium_html ["htm"];
    "ini" => arborium_ini ["cfg", "conf"];
    "java" => arborium_java [];
    "javascript" => arborium_javascript ["js", "mjs", "cjs", "node", "jsx"];
    "json" => arborium_json ["jsonc"];
    "just" => arborium_just ["justfile"] files ["justfile"];
    "kotlin" => arborium_kotlin ["kt", "kts"];
    "lua" => arborium_lua [];
    "make" => arborium_make ["makefile"] files ["makefile", "gnumakefile"];
    "markdown" => arborium_markdown ["md", "mdx"];
    "meson" => arborium_meson [] files ["meson.build", "meson_options.txt"];
    "ninja" => arborium_ninja [] files ["build.ninja"];
    "nix" => arborium_nix [] files ["flake.nix"];
    "objc" => arborium_objc ["objective-c", "objectivec", "mm"] ext ["m"];
    "ocaml" => arborium_ocaml ["ml", "mli"];
    "perl" => arborium_perl ["pl", "pm"];
    "php" => arborium_php [];
    "powershell" => arborium_powershell ["pwsh", "ps1", "psm1"];
    "proto" => arborium_proto ["protobuf"];
    "python" => arborium_python ["py", "python3", "py3"];
    "r" => arborium_r ["rlang"];
    "rego" => arborium_rego ["opa"];
    "ruby" => arborium_ruby ["rb"];
    "rust" => arborium_rust ["rs"];
    "scala" => arborium_scala ["sc"] files ["build.sbt"];
    "scheme" => arborium_scheme ["scm", "ss", "rkt"];
    "scss" => arborium_scss ["sass"];
    "solidity" => arborium_solidity ["sol"];
    "sql" => arborium_sql ["mysql", "postgres", "postgresql", "sqlite"];
    "starlark" => arborium_starlark ["bzl", "bazel"] files ["workspace"];
    "svelte" => arborium_svelte [];
    "swift" => arborium_swift [];
    "toml" => arborium_toml [];
    "tsx" => arborium_tsx [];
    "typescript" => arborium_typescript ["ts", "mts", "cts"];
    "vue" => arborium_vue [];
    "x86asm" => arborium_x86asm ["x86-asm", "nasm", "x86"];
    "xml" => arborium_xml ["xhtml", "svg", "xsl", "xslt"];
    "yaml" => arborium_yaml ["yml"];
    "zig" => arborium_zig [];
    "zsh" => arborium_zsh [] files [".zshrc"];
}

pub fn resolve_language(hint: &str, source: &str) -> Option<&'static str> {
    resolve_hint(hint)
        .or_else(|| shebang_id(source))
        .map(LanguageId::name)
}

pub(crate) fn resolve_hint(hint: &str) -> Option<LanguageId> {
    let hint = hint.trim();
    if hint.is_empty() {
        return None;
    }
    let normalized = if hint
        .bytes()
        .any(|byte| byte == b'\\' || byte.is_ascii_uppercase())
    {
        Cow::Owned(hint.replace('\\', "/").to_ascii_lowercase())
    } else {
        Cow::Borrowed(hint)
    };
    let file = normalized.rsplit('/').next().unwrap_or(&normalized);
    let extension = file.rsplit_once('.').map(|(_, extension)| extension);
    LanguageId::find(|language| language.aliases.contains(&normalized.as_ref()))
        .or_else(|| {
            let extension = extension?;
            LanguageId::find(|language| {
                language.aliases.contains(&extension) || language.extensions.contains(&extension)
            })
        })
        .or_else(|| LanguageId::find(|language| language.files.contains(&file)))
}

pub(crate) fn shebang_id(source: &str) -> Option<LanguageId> {
    let line = source.lines().next()?;
    if !line.starts_with("#!") {
        return None;
    }
    let lower = line.to_ascii_lowercase();
    let name = if lower.contains("python") {
        "python"
    } else if lower.contains("node") {
        "javascript"
    } else if lower.contains("zsh") {
        "zsh"
    } else if lower.contains("bash") || lower.contains("/sh") {
        "bash"
    } else if lower.contains("fish") {
        "fish"
    } else if lower.contains("pwsh") || lower.contains("powershell") {
        "powershell"
    } else {
        return None;
    };
    LanguageId::named(name)
}
