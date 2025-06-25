use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use indoc::indoc;
use toml_edit::{Array, DocumentMut, Item, Table};

use crate::args::InstallCmd;

pub(crate) fn install(cmd: InstallCmd) -> Result<()> {
    let config = InstallConfig::from(cmd);
    edit_book_toml(&config).context("cannot edit book.toml")?;
    write_css(&config).context("cannot install css")?;
    Ok(())
}

struct InstallConfig {
    book_root_dir: PathBuf,
    css_path: PathBuf,
}

impl From<InstallCmd> for InstallConfig {
    fn from(cmd: InstallCmd) -> Self {
        let InstallCmd {
            book_root_dir,
            css_dir,
        } = cmd;
        let css_path = css_dir.join("diataxis.css");
        Self {
            book_root_dir,
            css_path,
        }
    }
}

fn edit_book_toml(config: &InstallConfig) -> Result<()> {
    let InstallConfig {
        book_root_dir,
        css_path,
    } = config;
    let mut changed = false;

    let book_path = book_root_dir.join("book.toml");
    let mut book_toml = fs::read_to_string(&book_path)
        .with_context(|| anyhow!("Cannot read {}", book_path.display()))?
        .parse::<DocumentMut>()?;

    let output_table = book_toml
        .entry("output")
        .or_insert_with(|| {
            changed = true;
            implicit_table()
        })
        .as_table_mut()
        .ok_or_else(|| anyhow!("`output` entry must be a table"))?;
    let html_table = output_table
        .entry("html")
        .or_insert_with(|| {
            changed = true;
            implicit_table()
        })
        .as_table_mut()
        .ok_or_else(|| anyhow!("`output.html` entry must be a table"))?;
    let additional_css_array = html_table
        .entry("additional-css")
        .or_insert_with(|| {
            changed = true;
            Array::new().into()
        })
        .as_array_mut()
        .ok_or_else(|| anyhow!("`output.html.additional-css` must be an array"))?;
    if !additional_css_array.iter().any(|entry| {
        entry
            .as_str()
            .is_some_and(|entry_str| entry_str == css_path.as_os_str())
    }) {
        changed = true;
        additional_css_array.push(css_path.to_string_lossy().as_ref());
    }

    let preprocessor_table = book_toml
        .entry("preprocessor")
        .or_insert_with(|| {
            changed = true;
            implicit_table()
        })
        .as_table_mut()
        .ok_or_else(|| anyhow!("`preprocessor` entry must be a table"))?;
    let diataxis_item = preprocessor_table.entry("diataxis").or_insert_with(|| {
        changed = true;
        Item::Table(Table::new())
    });
    if !diataxis_item.is_table() {
        eprintln!("Warning: preprocessor.diataxis is not a table");
    }

    if changed {
        fs::write(&book_path, book_toml.to_string())
            .with_context(|| anyhow!("Cannot write {}", book_path.display()))?;
    }

    Ok(())
}

fn implicit_table() -> Item {
    let mut table = Table::new();
    table.set_implicit(true);
    Item::Table(table)
}

fn write_css(cmd: &InstallConfig) -> Result<()> {
    let InstallConfig {
        book_root_dir,
        css_path,
    } = cmd;
    write_file(
        book_root_dir.join(css_path),
        indoc! {"
            .diataxis-card-header {
                font-weight: bold;
                margin-top: 0ex;
                margin-bottom: 0ex;
            }

            .quote-grid {
                display: grid;
                gap: 3.55ex;
                grid-template-columns: repeat(auto-fit, minmax(330px, 1fr));
                margin: 3.55ex 0;
            }

            .quote-grid > blockquote {
                margin: 0;
            }
        "},
    )?;
    Ok(())
}

pub(crate) fn write_file(path: impl AsRef<Path>, content: impl AsRef<str>) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| anyhow!("cannot create parent directory of {}", path.display()))?;
    }

    fs::write(path, content.as_ref())
        .with_context(|| anyhow!("cannot write to {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use googletest::{
        expect_that,
        matchers::{all, contains_substring, eq},
    };
    use insta::assert_snapshot;

    use super::*;

    #[googletest::test]
    fn default() {
        let tempdir = tempfile::tempdir().unwrap();

        let book_toml_path = tempdir.path().join("book.toml");
        write_file(&book_toml_path, "").unwrap();

        install(InstallCmd {
            book_root_dir: tempdir.path().to_owned(),
            css_dir: PathBuf::from("theme/css"),
        })
        .unwrap();

        let book_toml_content = fs::read_to_string(&book_toml_path).unwrap();
        expect_that!(
            book_toml_content,
            all! {
                contains_substring("[preprocessor.diataxis]"),
                contains_substring("[output.html]"),
                contains_substring("additional-css = ["),
                contains_substring("theme/css/diataxis.css"),
            }
        );
        assert_snapshot!(book_toml_content);

        let diataxis_css_content =
            fs::read_to_string(tempdir.path().join("theme/css").join("diataxis.css")).unwrap();
        expect_that!(
            diataxis_css_content,
            contains_substring(".diataxis-card-header")
        );
        assert_snapshot!(diataxis_css_content);

        // Repeat installation has no additional effect.
        install(InstallCmd {
            book_root_dir: tempdir.path().to_owned(),
            css_dir: PathBuf::from("theme/css"),
        })
        .unwrap();
        let book_toml_content = fs::read_to_string(&book_toml_path).unwrap();
        expect_that!(
            book_toml_content.matches("[preprocessor.diataxis]").count(),
            eq(1)
        );
        expect_that!(
            book_toml_content.matches("theme/css/diataxis.css").count(),
            eq(1)
        );
    }
}
