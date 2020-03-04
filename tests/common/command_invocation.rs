use std::path::Path;
use std::process::ExitStatus;

pub(crate) struct Context<'a> {
    pub cli_params: &'a [&'a str],
    pub dir: &'a Path,
}

pub struct Output {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

impl<'a> Context<'a> {
    pub fn run(&mut self) -> Result<Output, failure::Error> {
        let mut command = test_bin::get_test_bin("cargo-bisect-rustc");
        for param in self.cli_params {
            command.arg(param);
        }
        let dir = self.dir;
        println!(
            "running `{:?} {}` in {:?}",
            command,
            self.cli_params.join(" "),
            dir.display()
        );
        assert!(dir.exists());
        let output = command.current_dir(dir).output()?;

        let stderr = String::from_utf8_lossy(&output.stderr);

        // prepass over the captured stdout, which by default emits a lot of
        // progressive info about downlaods that is fine in interactive settings
        // but just makes for a lot of junk when you want to understand the
        // final apparent output.
        let mut stdout = String::with_capacity(output.stdout.len());
        let mut line = String::new();
        for c in &output.stdout {
            match *c as char {
                '\r' => line.clear(),
                '\n' => {
                    stdout.push_str(&line);
                    line.clear();
                }
                c => line.push(c),
            }
        }
        stdout.push_str(&line);

        Ok(Output {
            status: output.status,
            stderr: stderr.to_string(),
            stdout,
        })
    }
}
