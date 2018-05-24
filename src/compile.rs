use Config;

use build_plan::{BuildPlan, TargetKind};
use std::error::Error;
use std::ffi::OsString;
use std::process::Command;
use std::path::{Path, PathBuf};

/// Commandline invocation blueprint for compiling tests like Cargo would.
///
/// This is obtained once at the start by hooking into Cargo.
#[derive(Debug, Clone)]
pub struct Blueprint {
    /// Compiler executable.
    program: String,
    /// Compiler arguments.
    args: Vec<OsString>,
    /// Index in `args` to replace with the source file we want to compile.
    source_file_index: usize,

    out_dir: Option<PathBuf>,
}

impl Blueprint {
    /// Obtains a `Blueprint` by attempting to compile the wrapper test with Cargo.
    pub fn obtain(config: &Config) -> Result<Self, Box<Error>> {
        // FIXME make `env!("CARGO")` configurable
        let output = Command::new(env!("CARGO"))
            .arg("-Zunstable-options")
            .arg("build")
            .arg("--build-plan")
            .arg("--test")
            .arg(Path::new(config.wrapper_test).file_stem().ok_or(format!("invalid `wrapper_test`"))?)
            .output()?;

        if !output.status.success() {
            return Err(format!("failed to obtain build plan from Cargo ({}): {}", output.status, String::from_utf8_lossy(&output.stderr)).into());
        }

        let raw_plan = output.stdout;

        let plan = BuildPlan::from_cargo_output(raw_plan)?;
        let invocations = plan.invocations.iter().filter(|inv| inv.target_kind == TargetKind::Test).collect::<Vec<_>>();
        assert_eq!(invocations.len(), 1);
        let invocation = invocations[0];

        // Extract arguments, replacing the arg containing `compile-fail.rs` with whatever we want
        // to compile. Congratulations, now we know how to build any test.
        // Additionally, remove `--test` to get a better default for compile-fail tests.
        let args = invocation.args.iter()
            .cloned()
            .filter(|arg| arg != "--test")
            .map(OsString::from)
            .collect::<Vec<_>>();

        // Find `compile-fail.rs`, ensuring that the match is unique
        let source_file_index = {
            let matches = args.iter()
                .enumerate()
                .filter_map(|(i, arg)| {
                    if arg == config.wrapper_test {
                        Some((i, arg))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();

            if matches.is_empty() {
                return Err("couldn't find wrapper test path in compiler command line".into());
            }
            if matches.len() > 1 {
                return Err(format!(
                    "found multiple arguments containing the wrapper test path in compiler command line: {}",
                    matches.iter()
                        .map(|&(i, arg)| format!("argument #{} ({})", i + 1, arg.to_str().unwrap()))
                        .collect::<Vec<_>>()
                        .join(", ")
                ).into());
            }

            matches[0].0
        };

        Ok(Blueprint {
            program: invocation.program.clone(),
            args,
            source_file_index,
            out_dir: None,
        })
    }

    pub fn set_out_dir(&mut self, out_dir: PathBuf) {
        self.out_dir = Some(out_dir);
    }

    /// Builds a `Command` that invokes rustc to compile the file `source`.
    pub fn build_command(&self, source: &Path) -> Command {
        let mut cmd = Command::new(&self.program);
        let mut out_dir = false;
        cmd.args(self.args.iter()
            .enumerate()
            .map(|(i, arg)| if i == self.source_file_index {
                source.as_os_str()
            } else if out_dir && self.out_dir.is_some() {
                out_dir = false;
                self.out_dir.as_ref().unwrap().as_os_str()
            } else {
                if arg == "--out-dir" {
                    out_dir = true;
                }
                arg.as_os_str()
            })
        );
        cmd
    }
}
