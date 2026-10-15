use std::{ffi::OsString, path::PathBuf};

#[derive(Debug, Default, PartialEq)]
pub(crate) struct LaunchOptions {
    pub inspect_local: bool,
    pub test_workspace: Option<PathBuf>,
}

impl LaunchOptions {
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self, String> {
        let mut options = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            if arg == "--inspect-local" {
                options.inspect_local = true;
            } else if arg == "--test-workspace" {
                let path = PathBuf::from(args.next().ok_or("--test-workspace 缺少目录")?);
                if !path.is_absolute() {
                    return Err("测试工作区必须是绝对路径".into());
                }
                options.test_workspace = Some(path);
            }
        }
        if options.test_workspace.is_some() && !options.inspect_local {
            return Err("--test-workspace 仅能与 --inspect-local 一起用于本地测试".into());
        }
        Ok(options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_launch_keeps_user_data_location() {
        assert_eq!(LaunchOptions::parse([]).unwrap(), LaunchOptions::default());
    }

    #[test]
    fn isolated_tests_require_explicit_debug_mode_and_absolute_path() {
        let dir = std::env::temp_dir().join("MUSP 独立测试");
        assert!(LaunchOptions::parse(["--test-workspace".into(), dir.clone().into()]).is_err());
        assert!(
            LaunchOptions::parse(["--inspect-local".into(), "--test-workspace".into()]).is_err()
        );
        assert!(
            LaunchOptions::parse([
                "--inspect-local".into(),
                "--test-workspace".into(),
                "relative".into()
            ])
            .is_err()
        );
        let options = LaunchOptions::parse([
            "--inspect-local".into(),
            "--test-workspace".into(),
            dir.clone().into(),
        ])
        .unwrap();
        assert!(options.inspect_local);
        assert_eq!(options.test_workspace, Some(dir));
    }
}
