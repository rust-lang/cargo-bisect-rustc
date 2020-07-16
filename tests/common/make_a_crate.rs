use std::borrow::Cow;
use std::fs::{DirBuilder, File};
use std::io::{Write};
use std::path::{Path};

type Text<'a> = Cow<'a, str>;

pub struct Crate<'a> {
    pub dir: &'a Path,
    pub name: &'a str,
    pub build_rs: Option<Text<'a>>,
    pub cargo_toml: Text<'a>,
    pub main_rs: Text<'a>,
}

impl<'a> Crate<'a> {
    pub fn make_files(&self, dir_builder: &DirBuilder) -> Result<(), failure::Error> {
        let dir = self.dir;
        let cargo_toml_path = dir.join("Cargo.toml");
        let build_path = dir.join("build.rs");
        let src_path = dir.join("src");
        let main_path = src_path.join("main.rs");

        dir_builder.create(&dir)?;
        dir_builder.create(src_path)?;

        if let Some(build_rs) = &self.build_rs {
            let mut build_file = File::create(build_path)?;
            writeln!(build_file, "{}", build_rs)?;
            build_file.sync_data()?;
        }

        let mut cargo_toml_file = File::create(cargo_toml_path)?;
        writeln!(cargo_toml_file, "{}", self.cargo_toml)?;
        cargo_toml_file.sync_data()?;

        let mut main_file = File::create(main_path)?;
        writeln!(main_file, "{}", self.main_rs)?;
        main_file.sync_data()?;

        Ok(())
    }
}
