use std::iter;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

use aho_corasick::{AhoCorasick, MatchKind};
use anyhow::{Context, Result, anyhow};
use indoc::writedoc;
use mdbook::BookItem;
use mdbook::book::{Book, Chapter};
use mdbook::errors::Result as MdbookResult;
use mdbook::preprocess::{Preprocessor, PreprocessorContext};
use pulldown_cmark::{Event, Parser};
use toml::value::Table;

#[derive(Default)]
pub struct DiataxisPreprocessor;

impl DiataxisPreprocessor {
    pub fn new() -> Self {
        Self
    }

    fn preprocess_bookitem(&self, item: &mut BookItem, config: &Config) -> Result<()> {
        match item {
            BookItem::Chapter(chapter) => self.preprocess_chapter(chapter, config),
            BookItem::Separator | BookItem::PartTitle(_) => Ok(()),
        }
    }

    fn preprocess_chapter(&self, chapter: &mut Chapter, config: &Config) -> Result<()> {
        let parser = Parser::new(&chapter.content).map(|event| match event {
            Event::Text(text) => Event::Text(self.preprocess_text(&text, config, &*chapter).into()),
            _ => event,
        });
        let new_content_capacity = (chapter.content.len() as f64 * 1.05) as usize;
        let mut new_content = String::with_capacity(new_content_capacity);
        pulldown_cmark_to_cmark::cmark(parser, &mut new_content)?;
        chapter.content = new_content;

        for sub_item in &mut chapter.sub_items {
            self.preprocess_bookitem(sub_item, config)?;
        }

        Ok(())
    }

    fn preprocess_text(&self, text: &str, config: &Config, chapter: &Chapter) -> String {
        static MATCHER: LazyLock<AhoCorasick> = LazyLock::new(|| {
            AhoCorasick::builder()
                .match_kind(MatchKind::LeftmostLongest)
                .build(Replacement::patterns())
                .unwrap()
        });

        let replacement_ctx = ReplacementCtx { config, chapter };
        let mut ret = String::with_capacity(text.len());
        MATCHER.replace_all_with(text, &mut ret, |result, _, ret| {
            let replacement = Replacement::from_pattern_index(result.pattern().as_usize());
            replacement.write_to(ret, &replacement_ctx);
            if replacement.is_malformed() {
                eprintln!(
                    "Warning: malformed `{{{{#diataxis ...}}}}` expression in {}",
                    chapter
                        .source_path
                        .as_deref()
                        .expect("internal error: draft chapter has content")
                        .display(),
                )
            }
            true
        });
        ret
    }
}

impl Preprocessor for DiataxisPreprocessor {
    fn name(&self) -> &str {
        "mdbook-diataxis"
    }

    fn supports_renderer(&self, renderer: &str) -> bool {
        renderer == "html"
    }

    fn run(&self, ctx: &PreprocessorContext, mut book: Book) -> MdbookResult<Book> {
        const CONFIG_KEY: &str = "diataxis";
        let config = ctx
            .config
            .get_preprocessor(CONFIG_KEY)
            .map(Config::new)
            .transpose()?
            .unwrap_or_default();

        for section in &mut book.sections {
            self.preprocess_bookitem(section, &config)?;
        }

        Ok(book)
    }
}

#[derive(Debug, Default)]
struct Config<'cfg> {
    tutorials: SectionConfig<'cfg>,
    how_to_guides: SectionConfig<'cfg>,
    reference: SectionConfig<'cfg>,
    explanation: SectionConfig<'cfg>,
}

impl<'cfg> Config<'cfg> {
    fn new(raw: &'cfg Table) -> Result<Self> {
        let section_overrides = |section| -> Result<SectionConfig<'_>> {
            let overrides = raw
                .get("compass")
                .map(|compass_value| {
                    compass_value
                        .as_table()
                        .ok_or_else(|| anyhow!("`compass` field must be a table"))
                })
                .transpose()?
                .and_then(|compass_table| compass_table.get(section))
                .map(|section_value| {
                    section_value
                        .as_table()
                        .ok_or_else(|| anyhow!("`compass.{section}` field must be a table"))
                })
                .transpose()?
                .map(|section_table| {
                    SectionConfig::new(section_table)
                        .with_context(|| anyhow!("cannot parse `compass.{section}` table"))
                })
                .transpose()?
                .unwrap_or_default();
            Ok(overrides)
        };
        let tutorials = section_overrides("tutorials")?;
        let how_to_guides = section_overrides("how-to-guides")?;
        let explanation = section_overrides("explanation")?;
        let reference = section_overrides("reference")?;
        Ok(Self {
            tutorials,
            how_to_guides,
            explanation,
            reference,
        })
    }

    fn tutorials_title(&self) -> &str {
        self.tutorials.title_override.unwrap_or("Tutorials")
    }

    fn tutorials_description(&self) -> &str {
        self.tutorials
            .description_override
            .unwrap_or("Hands-on lessons")
    }

    fn tutorials_link(&self) -> &Path {
        self.tutorials
            .link_override
            .as_deref()
            .unwrap_or(Path::new("./tutorials/index.html"))
    }

    fn how_to_guides_title(&self) -> &str {
        self.how_to_guides.title_override.unwrap_or("How-to guides")
    }

    fn how_to_guides_description(&self) -> &str {
        self.how_to_guides
            .description_override
            .unwrap_or("Step-by-step instructions for common tasks")
    }

    fn how_to_guides_link(&self) -> &Path {
        self.how_to_guides
            .link_override
            .as_deref()
            .unwrap_or(Path::new("./how-to/index.html"))
    }

    fn explanation_title(&self) -> &str {
        self.explanation.title_override.unwrap_or("Explanation")
    }

    fn explanation_description(&self) -> &str {
        self.explanation
            .description_override
            .unwrap_or("Long-form discussion of key topics")
    }

    fn explanation_link(&self) -> &Path {
        self.explanation
            .link_override
            .as_deref()
            .unwrap_or(Path::new("./explanations/index.html"))
    }

    fn reference_title(&self) -> &str {
        self.reference.title_override.unwrap_or("Reference")
    }

    fn reference_description(&self) -> &str {
        self.reference
            .description_override
            .unwrap_or("Technical information")
    }

    fn reference_link(&self) -> &Path {
        self.reference
            .link_override
            .as_deref()
            .unwrap_or(Path::new("./reference-materials/index.html"))
    }
}

#[derive(Debug, Default)]
struct SectionConfig<'cfg> {
    title_override: Option<&'cfg str>,
    description_override: Option<&'cfg str>,
    link_override: Option<PathBuf>,
}

impl<'cfg> SectionConfig<'cfg> {
    fn new(config_table: &'cfg Table) -> Result<Self> {
        let title_override = config_table
            .get("title")
            .map(|title| {
                title
                    .as_str()
                    .ok_or_else(|| anyhow!("`title` field must be a string"))
            })
            .transpose()?;
        let description_override = config_table
            .get("description")
            .map(|desc| {
                desc.as_str()
                    .ok_or_else(|| anyhow!("`description` field must be a string"))
            })
            .transpose()?;
        let link_override = config_table
            .get("link")
            .map(|file| {
                file.as_str()
                    .ok_or_else(|| anyhow!("`link` field must be a string"))
            })
            .transpose()?
            .map(Path::new)
            .map(|path| {
                if path
                    .file_name()
                    .is_some_and(|file_name| file_name == "README.md")
                {
                    return path.with_file_name("index.html");
                }
                path.to_owned()
            })
            .map(|mut path| {
                path.set_extension("html");
                path
            });
        Ok(Self {
            title_override,
            description_override,
            link_override,
        })
    }
}

#[derive(Copy, Clone)]
enum Replacement {
    Compass,
    Toc,
    Malformed,
}

impl Replacement {
    const fn patterns() -> [&'static str; 3] {
        [
            Self::Compass.pattern(),
            Self::Toc.pattern(),
            Self::Malformed.pattern(),
        ]
    }

    const fn pattern(&self) -> &'static str {
        match self {
            Self::Compass => "{{#diataxis compass}}",
            Self::Toc => "{{#diataxis table-of-contents}}",
            Self::Malformed => "{{#diataxis",
        }
    }

    fn from_pattern_index(index: usize) -> Self {
        [Self::Compass, Self::Toc, Self::Malformed][index]
    }

    fn is_malformed(&self) -> bool {
        matches!(self, Self::Malformed)
    }

    fn write_to(&self, buf: &mut String, ctx: &ReplacementCtx) {
        match self {
            Self::Compass => self.write_compass_to(buf, ctx),
            Self::Toc => self.write_toc_to(buf, ctx),
            Self::Malformed => buf.push_str(self.pattern()),
        };
    }

    fn write_compass_to(&self, buf: &mut String, ctx: &ReplacementCtx) {
        use std::fmt::Write;

        let tutorials_title = ctx.config.tutorials_title();
        let tutorials_description = ctx.config.tutorials_description();
        let tutorials_link = ctx.config.tutorials_link().display();
        let how_to_guide_title = ctx.config.how_to_guides_title();
        let how_to_guide_description = ctx.config.how_to_guides_description();
        let how_to_guides_link = ctx.config.how_to_guides_link().display();
        let reference_title = ctx.config.reference_title();
        let reference_description = ctx.config.reference_description();
        let reference_link = ctx.config.reference_link().display();
        let explanation_title = ctx.config.explanation_title();
        let explanation_description = ctx.config.explanation_description();
        let explanation_link = ctx.config.explanation_link().display();
        writedoc!(
            buf,
            // TODO(kcza): this &#8288; causes spacing issues but otherwise if tje
            // snippet starts with a `<`, it gets escaped, ruining the outermost html
            // tags.
            r#"
                &#8288;<div class="quote-grid">
                    <blockquote>
                        <p>
                            <div class="diataxis-card-header">
                                <a href="{tutorials_link}">{tutorials_title}</a>
                            </div>
                            {tutorials_description}
                        </p>
                    </blockquote>
                    <blockquote>
                        <p>
                            <div class="diataxis-card-header">
                                <a href="{how_to_guides_link}">{how_to_guide_title}</a>
                            </div>
                            {how_to_guide_description}
                        </p>
                    </blockquote>
                    <blockquote>
                        <p>
                            <div class="diataxis-card-header">
                                <a href="{explanation_link}">{explanation_title}</a>
                            </div>
                            {explanation_description}
                        </p>
                    </blockquote>
                    <blockquote>
                        <p>
                            <div class="diataxis-card-header">
                                <a href="{reference_link}">{reference_title}</a>
                            </div>
                            {reference_description}
                        </p>
                    </blockquote>
                </div>
            "#,
        )
        .expect("internal error: cannot to write to string");
    }

    fn write_toc_to(&self, buf: &mut String, ctx: &ReplacementCtx) {
        let chapter_path = match &ctx.chapter.source_path {
            Some(path) => path,
            _ => return,
        };
        ctx.chapter
            .sub_items
            .iter()
            .filter_map(|item| match item {
                BookItem::Chapter(chapter) => Some(chapter),
                _ => None,
            })
            .for_each(|child| {
                use std::fmt::Write;
                let name = &child.name;
                let link_path = child
                    .source_path
                    .as_deref()
                    .map(|path| relative_to(chapter_path, path));
                if let Some(link_path) = link_path {
                    writeln!(buf, "- [{name}]({})", link_path.display())
                        .expect("internal error: cannot to write to string")
                } else {
                    writeln!(buf, "- {name}").expect("internal error: cannot to write to string")
                }
            });
    }
}

/// Computes the path of `target` relative to `source`.
///
/// `target` must be a sibling of `source` or be in a child directory which is a sibling of
/// `source`. Symlinks are not supported.
fn relative_to(source: &Path, target: &Path) -> PathBuf {
    target
        .components()
        .zip(source.components().chain(iter::repeat(Component::RootDir)))
        .skip_while(|(target_component, source_component)| target_component == source_component)
        .map(|(target_component, _)| target_component)
        .collect::<PathBuf>()
}

struct ReplacementCtx<'ctx> {
    #[allow(unused)]
    config: &'ctx Config<'ctx>,
    #[allow(unused)]
    chapter: &'ctx Chapter,
}

#[cfg(test)]
mod tests {
    use super::*;

    use googletest::expect_that;
    use googletest::matchers::{all, contains_substring};
    use indoc::indoc;
    use insta::assert_snapshot;
    use mdbook::preprocess::CmdPreprocessor;

    mod compass {
        use super::*;

        #[googletest::test]
        fn default() {
            let input_json = indoc! {br##"
                [{
                    "root": "/path/to/book",
                    "config": {
                        "book": {
                            "authors": ["AUTHOR"],
                            "language": "en",
                            "multilingual": false,
                            "src": "src",
                            "title": "TITLE"
                        },
                        "preprocessor": {
                            "diataxis": {}
                        }
                    },
                    "renderer": "html",
                    "mdbook_version": "0.4.21"
                }, {
                    "sections": [{
                        "Chapter": {
                            "name": "Chapter 1",
                            "content": "# Chapter 1\n{{#diataxis compass}}",
                            "number": [1],
                            "sub_items": [],
                            "path": "chapter_1.md",
                            "source_path": "chapter_1.md",
                            "parent_names": []
                        }
                    }],
                    "__non_exhaustive": null
                }]
            "##};
            let (ctx, book) = CmdPreprocessor::parse_input(&input_json[..]).unwrap();
            let book = DiataxisPreprocessor::new().run(&ctx, book).unwrap();
            let chapter = match &book.sections[0] {
                BookItem::Chapter(chapter) => chapter,
                _ => panic!("unexpected first item"),
            };
            expect_that!(
                chapter.content,
                all!(
                    contains_substring("Tutorials"),
                    contains_substring("How-to guides"),
                    contains_substring("Reference"),
                    contains_substring("Explanation"),
                )
            );
            assert_snapshot!(chapter.content);
        }

        #[googletest::test]
        fn configured() {
            let input_json = indoc! {br##"
                [{
                    "root": "/path/to/book",
                    "config": {
                        "book": {
                            "authors": ["AUTHOR"],
                            "language": "en",
                            "multilingual": false,
                            "src": "src",
                            "title": "TITLE"
                        },
                        "preprocessor": {
                            "diataxis": {
                                "compass": {
                                    "tutorials": {
                                        "title": "custom-tutorials-title",
                                        "description": "custom-tutorials-description",
                                        "link": "custom-tutorials/README.md"
                                    },
                                    "how-to-guides": {
                                        "title": "custom-how-to-guides-title",
                                        "description": "custom-how-to-guides-description",
                                        "link": "custom-how-to-guides-link.md"
                                    },
                                    "reference": {
                                        "title": "custom-reference-title",
                                        "description": "custom-reference-description",
                                        "link": "custom-reference-link.md"
                                    },
                                    "explanation": {
                                        "title": "custom-explanation-title",
                                        "description": "custom-explanation-description",
                                        "link": "custom-explanation-link.md"
                                    }
                                }
                            }
                        }
                    },
                    "renderer": "html",
                    "mdbook_version": "0.4.21"
                }, {
                    "sections": [{
                        "Chapter": {
                            "name": "Chapter 1",
                            "content": "# Chapter 1\n{{#diataxis compass}}",
                            "number": [1],
                            "sub_items": [],
                            "path": "chapter_1.md",
                            "source_path": "chapter_1.md",
                            "parent_names": []
                        }
                    }],
                    "__non_exhaustive": null
                }]
            "##};
            let (ctx, book) = CmdPreprocessor::parse_input(&input_json[..]).unwrap();
            let book = DiataxisPreprocessor::new().run(&ctx, book).unwrap();
            let chapter = match &book.sections[0] {
                BookItem::Chapter(chapter) => chapter,
                _ => panic!("unexpected first item"),
            };
            expect_that!(
                chapter.content,
                all!(
                    contains_substring("custom-tutorials-title"),
                    contains_substring("custom-tutorials-description"),
                    contains_substring(r#"href="custom-tutorials/index.html""#),
                    contains_substring("custom-how-to-guides-title"),
                    contains_substring("custom-how-to-guides-description"),
                    contains_substring(r#"href="custom-how-to-guides-link.html""#),
                    contains_substring("custom-reference-title"),
                    contains_substring("custom-reference-description"),
                    contains_substring(r#"href="custom-reference-link.html""#),
                    contains_substring("custom-explanation-title"),
                    contains_substring("custom-explanation-description"),
                    contains_substring(r#"href="custom-explanation-link.html""#),
                )
            );
            assert_snapshot!(chapter.content);
        }
    }

    mod toc {
        use super::*;

        #[googletest::test]
        fn default() {
            let input_json = indoc! {br##"
                [{
                    "root": "/path/to/book",
                    "config": {
                        "book": {
                            "authors": ["AUTHOR"],
                            "language": "en",
                            "multilingual": false,
                            "src": "src",
                            "title": "TITLE"
                        },
                        "preprocessor": {
                            "diataxis": {}
                        }
                    },
                    "renderer": "html",
                    "mdbook_version": "0.4.21"
                }, {
                    "sections": [{
                        "Chapter": {
                            "name": "Chapter 1",
                            "content": "# Chapter 1\n{{#diataxis table-of-contents}}",
                            "number": [1],
                            "sub_items": [{
                                "Chapter": {
                                    "name": "Non-draft sub-chapter",
                                    "content": "non-draft sub content",
                                    "number": [1, 1],
                                    "sub_items": [],
                                    "path": "chapter_1/dir/non_draft_sub.md",
                                    "source_path": "chapter_1/dir/non_draft_sub.md",
                                    "parent_names": []
                                }
                            }, {
                                "Chapter": {
                                    "name": "Draft sub-chapter",
                                    "content": "draft sub content",
                                    "number": [1, 1],
                                    "sub_items": [],
                                    "path": "chapter_1/dir/draft_sub.md",
                                    "parent_names": []
                                }
                            }],
                            "path": "chapter_1/README.md",
                            "source_path": "chapter_1/README.md",
                            "parent_names": []
                        }
                    }],
                    "__non_exhaustive": null
                }]
            "##};
            let (ctx, book) = CmdPreprocessor::parse_input(&input_json[..]).unwrap();
            let book = DiataxisPreprocessor::new().run(&ctx, book).unwrap();
            let chapter = match &book.sections[0] {
                BookItem::Chapter(chapter) => chapter,
                _ => panic!("unexpected first item"),
            };
            expect_that!(
                chapter.content,
                all!(
                    contains_substring("- [Non-draft sub-chapter](dir/non_draft_sub.md)"),
                    contains_substring("- Draft sub-chapter"),
                )
            );
            assert_snapshot!(chapter.content);
        }
    }
}
