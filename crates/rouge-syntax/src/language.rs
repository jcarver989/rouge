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
    ($($feature:literal: $name:literal => $module:ident [$($alias:literal),*] $(ext [$($ext:literal),*])? $(files [$($file:literal),*])?;)*) => {
        pub(crate) const LANGUAGES: &[Language] = &[
            $(
                #[cfg(feature = $feature)]
                Language {
                    name: $name,
                    aliases: &[$name, $($alias),*],
                    extensions: &[$($($ext),*)?],
                    files: &[$($($file),*)?],
                    spec: || -> Spec { ($module::language().into(), &$module::HIGHLIGHTS_QUERY, &$module::INJECTIONS_QUERY) },
                },
            )*
        ];
    };
}

languages! {
    "lang-asm": "asm" => lang_asm ["assembly"] ext ["s"];
    "lang-bash": "bash" => lang_bash ["sh", "shell"] files [".bashrc"];
    "lang-batch": "batch" => lang_batch ["bat", "cmd"];
    "lang-c": "c" => lang_c ["h"];
    "lang-c-sharp": "c-sharp" => lang_c_sharp ["csharp", "c#", "cs"];
    "lang-clojure": "clojure" => lang_clojure ["clj", "cljs", "cljc", "edn"] files ["deps.edn"];
    "lang-cmake": "cmake" => lang_cmake [] files ["cmakelists.txt"];
    "lang-commonlisp": "commonlisp" => lang_commonlisp ["common-lisp", "lisp", "cl"];
    "lang-cpp": "cpp" => lang_cpp ["c++", "cc", "cxx", "hpp", "hh", "hxx"];
    "lang-css": "css" => lang_css [];
    "lang-dart": "dart" => lang_dart [];
    "lang-diff": "diff" => lang_diff ["patch"];
    "lang-dockerfile": "dockerfile" => lang_dockerfile ["docker"] files ["dockerfile", "containerfile"];
    "lang-elixir": "elixir" => lang_elixir ["ex", "exs"];
    "lang-erlang": "erlang" => lang_erlang ["erl", "hrl"];
    "lang-fish": "fish" => lang_fish [];
    "lang-go": "go" => lang_go ["golang"] files ["go.mod", "go.sum"];
    "lang-graphql": "graphql" => lang_graphql ["gql"];
    "lang-haskell": "haskell" => lang_haskell ["hs"];
    "lang-hcl": "hcl" => lang_hcl ["terraform", "tf", "tfvars"] files [".terraformrc"];
    "lang-html": "html" => lang_html ["htm"];
    "lang-ini": "ini" => lang_ini ["cfg", "conf"];
    "lang-java": "java" => lang_java [];
    "lang-javascript": "javascript" => lang_javascript ["js", "mjs", "cjs", "node", "jsx"];
    "lang-json": "json" => lang_json ["jsonc"];
    "lang-just": "just" => lang_just ["justfile"] files ["justfile"];
    "lang-kotlin": "kotlin" => lang_kotlin ["kt", "kts"];
    "lang-lua": "lua" => lang_lua [];
    "lang-make": "make" => lang_make ["makefile"] files ["makefile", "gnumakefile"];
    "lang-markdown": "markdown" => lang_markdown ["md", "mdx"];
    "lang-meson": "meson" => lang_meson [] files ["meson.build", "meson_options.txt"];
    "lang-ninja": "ninja" => lang_ninja [] files ["build.ninja"];
    "lang-nix": "nix" => lang_nix [] files ["flake.nix"];
    "lang-objc": "objc" => lang_objc ["objective-c", "objectivec", "mm"] ext ["m"];
    "lang-ocaml": "ocaml" => lang_ocaml ["ml", "mli"];
    "lang-perl": "perl" => lang_perl ["pl", "pm"];
    "lang-php": "php" => lang_php [];
    "lang-powershell": "powershell" => lang_powershell ["pwsh", "ps1", "psm1"];
    "lang-proto": "proto" => lang_proto ["protobuf"];
    "lang-python": "python" => lang_python ["py", "python3", "py3"];
    "lang-r": "r" => lang_r ["rlang"];
    "lang-rego": "rego" => lang_rego ["opa"];
    "lang-ruby": "ruby" => lang_ruby ["rb"];
    "lang-rust": "rust" => lang_rust ["rs"];
    "lang-scala": "scala" => lang_scala ["sc"] files ["build.sbt"];
    "lang-scheme": "scheme" => lang_scheme ["scm", "ss", "rkt"];
    "lang-scss": "scss" => lang_scss ["sass"];
    "lang-solidity": "solidity" => lang_solidity ["sol"];
    "lang-sql": "sql" => lang_sql ["mysql", "postgres", "postgresql", "sqlite"];
    "lang-starlark": "starlark" => lang_starlark ["bzl", "bazel"] files ["workspace"];
    "lang-svelte": "svelte" => lang_svelte [];
    "lang-swift": "swift" => lang_swift [];
    "lang-toml": "toml" => lang_toml [];
    "lang-tsx": "tsx" => lang_tsx [];
    "lang-typescript": "typescript" => lang_typescript ["ts", "mts", "cts"];
    "lang-vue": "vue" => lang_vue [];
    "lang-x86asm": "x86asm" => lang_x86asm ["x86-asm", "nasm", "x86"];
    "lang-xml": "xml" => lang_xml ["xhtml", "svg", "xsl", "xslt"];
    "lang-yaml": "yaml" => lang_yaml ["yml"];
    "lang-zig": "zig" => lang_zig [];
    "lang-zsh": "zsh" => lang_zsh [] files [".zshrc"];
}

pub fn languages() -> impl Iterator<Item = &'static str> {
    LANGUAGES.iter().map(|language| language.name)
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
