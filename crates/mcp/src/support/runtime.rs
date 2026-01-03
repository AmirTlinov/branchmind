#![forbid(unsafe_code)]

use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Toolset {
    Full,
    Daily,
    Core,
}

impl Toolset {
    pub(crate) fn from_str(value: &str) -> Option<Self> {
        match value {
            "full" => Some(Self::Full),
            "daily" | "dx" => Some(Self::Daily),
            "core" | "minimal" => Some(Self::Core),
            _ => None,
        }
    }

    pub(crate) fn parse(value: Option<&str>) -> Self {
        value.and_then(Self::from_str).unwrap_or(Self::Full)
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Daily => "daily",
            Self::Core => "core",
        }
    }
}

pub(crate) fn parse_storage_dir() -> PathBuf {
    let mut args = std::env::args().skip(1);
    let mut storage_dir: Option<PathBuf> = None;
    while let Some(arg) = args.next() {
        if arg.as_str() == "--storage-dir"
            && let Some(value) = args.next()
        {
            storage_dir = Some(PathBuf::from(value));
        }
    }
    storage_dir.unwrap_or_else(|| PathBuf::from(".branchmind_rust"))
}

pub(crate) fn parse_toolset() -> Toolset {
    let mut args = std::env::args().skip(1);
    let mut cli: Option<String> = None;
    while let Some(arg) = args.next() {
        if arg.as_str() == "--toolset"
            && let Some(value) = args.next()
        {
            cli = Some(value);
            break;
        }
    }

    let value = cli.or_else(|| std::env::var("BRANCHMIND_TOOLSET").ok());
    Toolset::parse(value.as_deref())
}

pub(crate) fn parse_default_workspace() -> Option<String> {
    let mut args = std::env::args().skip(1);
    let mut cli: Option<String> = None;
    while let Some(arg) = args.next() {
        if arg.as_str() == "--workspace"
            && let Some(value) = args.next()
        {
            cli = Some(value);
            break;
        }
    }

    cli.or_else(|| std::env::var("BRANCHMIND_WORKSPACE").ok())
}
