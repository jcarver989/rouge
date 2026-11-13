use crate::tokens::ICON_SIZE;
use rouge_core::{BuildCx, Color, ElementId, IntoProp, Path, Prop, Styled, View, div, icon, view};

macro_rules! icons {
    ($($name:ident => $file:literal,)*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum IconName {
            $($name,)*
        }

        impl IconName {
            pub const ALL: &[Self] = &[$(Self::$name,)*];

            fn geometry(self) -> &'static str {
                match self {
                    $(Self::$name => include_str!(concat!("../assets/icons/", $file, ".path")),)*
                }
            }
        }
    };
}

icons! {
    Archive => "archive",
    ArchiveRestore => "archive-restore",
    ArrowDown => "arrow-down",
    ArrowLeft => "arrow-left",
    ArrowRight => "arrow-right",
    ArrowUp => "arrow-up",
    AudioLines => "audio-lines",
    Bell => "bell",
    Bot => "bot",
    Brain => "brain",
    Check => "check",
    ChevronDown => "chevron-down",
    ChevronLeft => "chevron-left",
    ChevronRight => "chevron-right",
    ChevronsDownUp => "chevrons-down-up",
    ChevronsUpDown => "chevrons-up-down",
    Circle => "circle",
    CircleCheck => "circle-check",
    CircleDot => "circle-dot",
    CircleSlash => "circle-slash",
    CircleX => "circle-x",
    Cloud => "cloud",
    Columns2 => "columns-2",
    Copy => "copy",
    Cpu => "cpu",
    Ellipsis => "ellipsis",
    Eye => "eye",
    File => "file",
    FileDiff => "file-diff",
    FileText => "file-text",
    Folder => "folder",
    FolderOpen => "folder-open",
    FolderPlus => "folder-plus",
    GitBranch => "git-branch",
    GitCommit => "git-commit-horizontal",
    GitCompare => "git-compare",
    GitPullRequest => "git-pull-request",
    Image => "image",
    Info => "info",
    Laptop => "laptop",
    LoaderCircle => "loader-circle",
    MessageCircle => "message-circle",
    MessageSquareText => "message-square-text",
    Minus => "minus",
    MoveHorizontal => "move-horizontal",
    Palette => "palette",
    PanelLeft => "panel-left",
    PanelRight => "panel-right",
    Paperclip => "paperclip",
    Pause => "pause",
    Pencil => "pencil",
    Pin => "pin",
    PinOff => "pin-off",
    Play => "play",
    Plus => "plus",
    Power => "power",
    RotateCw => "rotate-cw",
    Rows2 => "rows-2",
    Search => "search",
    Settings => "settings",
    Square => "square",
    SquarePen => "square-pen",
    SquareTerminal => "square-terminal",
    Timer => "timer",
    TriangleAlert => "triangle-alert",
    Unplug => "unplug",
    WrapText => "wrap-text",
    X => "x",
}

impl From<IconName> for Path {
    fn from(name: IconName) -> Self {
        Path::svg(name.geometry())
    }
}

pub struct Icon {
    path: Path,
    size: f32,
    color: Prop<Color>,
}

impl Icon {
    pub fn new<T>(icon: impl Into<Path>, color: impl IntoProp<Color, T>) -> Self {
        Self {
            path: icon.into(),
            size: ICON_SIZE,
            color: color.into_prop(),
        }
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }
}

impl View for Icon {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let color = self.color;
        view! {
            <div flex_none w=self.size h=self.size>
                <icon path=self.path color={move || color.get()} stroke_round=2.0 size_full/>
            </div>
        }
        .build(cx)
    }
}
