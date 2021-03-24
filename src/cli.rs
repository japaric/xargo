use std::env;

use cargo::Subcommand;

pub struct Args {
    all: Vec<String>,
    subcommand: Option<Subcommand>,
    target: Option<String>,
    message_format: Option<String>,
    manifest_path: Option<String>,  // path to the Cargo toml file given in --manifest-path
}

impl Args {
    pub fn all(&self) -> &[String] {
        &self.all
    }

    pub fn subcommand(&self) -> Option<Subcommand> {
        self.subcommand
    }

    pub fn target(&self) -> Option<&str> {
        self.target.as_ref().map(|s| &**s)
    }

    pub fn message_format(&self) -> Option<&str> {
        self.message_format.as_ref().map(|s| &**s)
    }

    pub fn verbose(&self) -> bool {
        self.all
            .iter()
            .any(|a| a == "--verbose" || a == "-v" || a == "-vv")
    }

    pub fn version(&self) -> bool {
        self.all.iter().any(|a| a == "--version" || a == "-V")
    }

    pub fn manifest_path(&self) -> Option<&str> {
        self.manifest_path.as_ref().map(|s| &**s)
    }
}

pub fn args() -> Args {
    let all = env::args().skip(1).collect::<Vec<_>>();

    let mut subcommand = None;
    let mut target = None;
    let mut message_format = None;
    let mut manifest_path = None;
    {
        let mut args = all.iter();
        while let Some(arg) = args.next() {
            if !arg.starts_with("-") {
                subcommand = subcommand.or_else(|| Some(Subcommand::from(&**arg)));
            }

            if arg == "--target" {
                target = args.next().map(|s| s.to_owned());
            } else if arg.starts_with("--target=") {
                target = arg.splitn(2, '=').nth(1).map(|s| s.to_owned());
            } else if arg == "--message-format" {
                message_format = args.next().map(|s| s.to_owned());
            } else if arg.starts_with("--message-format=") {
                message_format = arg.splitn(2, '=').nth(1).map(|s| s.to_owned());
            } else if arg.starts_with("--manifest-path") {
                manifest_path = args.next().map(|s| s.to_owned());
            }
        }
    }

    Args {
        all,
        subcommand,
        target,
        message_format,
        manifest_path,
    }
}
