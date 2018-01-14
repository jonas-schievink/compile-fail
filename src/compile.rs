use Config;

use cargo::ops::*;
use cargo::util::important_paths::find_project_manifest;
use cargo::util::process_builder::ProcessBuilder;
use cargo::util::config::Config as CargoConfig;
use cargo::util::errors::CargoResult;
use cargo::core::Workspace;
use cargo::core::package_id::PackageId;
use cargo::core::shell::Shell;
use cargo::core::manifest::{Target, TargetKind};
use std::env::current_dir;
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::ffi::OsString;
use std::process::Command;
use std::path::{Path, PathBuf};

/// Commandline invocation blueprint for compiling tests like Cargo would.
///
/// This is obtained once at the start by hooking into Cargo.
#[derive(Debug, Clone)]
pub struct Blueprint {
    /// Compiler executable.
    program: OsString,
    /// Compiler arguments.
    args: Vec<OsString>,
    /// Index in `args` to replace with the source file we want to compile.
    source_file_index: usize,

    out_dir: Option<PathBuf>,
}

impl Blueprint {
    /// Obtains a `Blueprint` by attempting to compile the wrapper test with Cargo.
    pub fn obtain(config: &Config) -> Result<Self, Box<Error>> {
        let cargo_config = CargoConfig::default()?;
        // direct Cargo's console output to a buffer
        *cargo_config.shell() = Shell::from_write(Box::new(Vec::new()));

        let cwd = current_dir()?;
        let mfst = find_project_manifest(&cwd, "Cargo.toml")?;
        let ws = Workspace::new(&mfst, &cargo_config)?;

        // configure Cargo to build the test that invokes `compile-fail`
        let testpath = Path::new(config.wrapper_test);
        let testname = testpath.file_stem()
            .ok_or(format!("`wrapper_test` must be a path to a file (is '{}')", config.wrapper_test))?;
        let testname = testname.to_str().unwrap().to_string();
        info!("wrapper test path: {}", testpath.display());
        info!("wrapper test name: {}", testname);

        let filter = [testname];
        let mut opt = CompileOptions::default(&cargo_config, CompileMode::Build);
        opt.filter = CompileFilter::Only {
            all_targets: false,
            lib: false,
            bins: FilterRule::Just(&[]),
            examples: FilterRule::Just(&[]),
            tests: FilterRule::Just(&filter),
            benches: FilterRule::Just(&[]),
        };

        let testpath = Path::new(config.wrapper_test);
        let exec = Arc::new(Exec {
            testpath: testpath.to_str().unwrap().to_string(),
            testname: testpath.file_stem()
                .ok_or(format!("wrapper_test must be a path to a test .rs file (is '{}')", testpath.display()))?
                .to_str().unwrap()
                .to_string(),
            found_test: AtomicBool::new(false),
            result: Mutex::new(Ok(None)),
        });

        compile_with_exec(&ws, &opt, exec.clone())?;

        exec.result()
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

struct Exec {
    /// Path to the wrapper test (eg. `tests/compile-fail.rs`).
    testpath: String,
    /// The wrapper test name from Cargo's point of view (eg. `compile-fail`).
    testname: String,
    /// Set to `true` when we found the wrapper test to recompile.
    found_test: AtomicBool,
    result: Mutex<Result<Option<Blueprint>, String>>,
}

impl Exec {
    /// Checks whether the test execution was successful.
    fn result(&self) -> Result<Blueprint, Box<Error>> {
        let result = self.result.lock().unwrap();
        match *result {
            Ok(Some(ref bp)) => Ok(bp.clone()),
            Ok(None) => Err("couldn't find wrapper test".into()),
            Err(ref e) => Err(e.to_string().into()),
        }
    }

    /// Mark the `Exec` as failed and store the error to report.
    fn error<E: ToString>(&self, err: E) {
        let mut result = self.result.lock().unwrap();
        if result.is_ok() {
            *result = Err(err.to_string());
        } else {
            info!("execution already failed, dropping subsequent error: {}", err.to_string());
        }
    }

    fn store_result(&self, blueprint: Blueprint) {
        let mut result = self.result.lock().unwrap();
        if let Ok(ref mut ok) = *result {
            if let Some(ref bp) = *ok {
                panic!("attempt to store duplicate result. first result is {:?}, new is {:?}", bp, blueprint);
            } else {
                *ok = Some(blueprint);
            }
        }
    }
}

impl Executor for Exec {
    fn exec(
        &self,
        cmd: ProcessBuilder,
        id: &PackageId,
        target: &Target
    ) -> CargoResult<()> {
        info!("exec called for package {}, target {}", id, target);
        info!("exec process = {}", cmd);

        // Extract arguments, replacing the arg containing `compile-fail.rs` with whatever we want
        // to compile. Congratulations, now we know how to build any test.
        // Additionally, remove `--test` to get a better default for compile-fail tests.
        let args = cmd.get_args().iter()
            .cloned()
            .filter(|arg| arg != "--test")
            .collect::<Vec<_>>();

        // Find `compile-fail.rs`, ensuring that the match is unique
        let source_file_index = {
            let matches = args.iter()
                .enumerate()
                .filter_map(|(i, arg)| {
                    match arg.to_str() {
                        Some(arg) if arg == self.testpath => Some((i, arg)),
                        _ => None,
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
                        .map(|&(i, arg)| format!("argument #{} ({})", i + 1, arg))
                        .collect::<Vec<_>>()
                        .join(", ")
                ).into());
            }

            matches[0].0
        };

        self.store_result(Blueprint {
            args,
            source_file_index,
            out_dir: None,
            program: cmd.get_program().clone(),
        });

        Ok(())
    }

    fn exec_json(
        &self,
        _cmd: ProcessBuilder,
        _id: &PackageId,
        _target: &Target,
        _handle_stdout: &mut FnMut(&str) -> CargoResult<()>,
        _handle_stderr: &mut FnMut(&str) -> CargoResult<()>
    ) -> CargoResult<()> {
        unimplemented!();
    }

    fn force_rebuild(&self, unit: &Unit) -> bool {
        debug!("force_rebuild of unit; target = {}, profile = {}", unit.target, unit.profile);
        match (unit.target.kind(), unit.target.name()) {
            (&TargetKind::Test, testname) if testname == self.testname => {
                info!("forcing rebuild of {} with profile {}", unit.target, unit.profile);
                if self.found_test.swap(true, Ordering::SeqCst) {
                    error!("already found a matching test, ambiguity!");
                    self.error(
                        "found multiple tests matching the configured test name, run with \
                        `RUST_LOG=compile_fail` to learn more"
                    );
                    false
                } else {
                    true
                }
            }
            _ => false,
        }
    }
}
