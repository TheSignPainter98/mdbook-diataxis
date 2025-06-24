use std::sync::LazyLock;

// TODO(kcza): error handling, e.g. on {{#diataxis unrecognised}}

use aho_corasick::AhoCorasick;
use indoc::indoc;
use mdbook::book::{Book, Chapter};
use mdbook::errors::Result;
use mdbook::preprocess::{Preprocessor, PreprocessorContext};
use mdbook::BookItem;
use pulldown_cmark::{Event, Parser};
use toml::value::Table;

pub(crate) struct DiataxisPreprocessor;

impl DiataxisPreprocessor {
    pub(crate) fn new() -> Self {
        Self
    }

    fn preprocess_bookitem(&self, item: &mut BookItem, config: &Config) -> Result<()> {
        match item {
            BookItem::Chapter(chapter) => self.preprocess_chapter(chapter, config),
            BookItem::Separator | BookItem::PartTitle(_) => Ok(()),
        }
    }

    fn preprocess_chapter(&self, chapter: &mut Chapter, config: &Config) -> Result<()> {
        println!("looking at {chapter:?}");

        let parser = Parser::new(&chapter.content).map(|event| match event {
            Event::Text(text) => Event::Text(self.preprocess_text(&text, config, &*chapter).into()),
            _ => event,
        });
        let new_content_capacity = (chapter.content.len() as f64 * 1.05) as usize;
        let mut new_content = String::with_capacity(new_content_capacity);
        pulldown_cmark_to_cmark::cmark(parser, &mut new_content)?;
        chapter.content = new_content;

        Ok(())
    }

    fn preprocess_text(&self, text: &str, config: &Config, chapter: &Chapter) -> String {
        static MATCHER: LazyLock<AhoCorasick> =
            LazyLock::new(|| AhoCorasick::new(Replacement::patterns()).unwrap());

        let replacement_ctx = ReplacementCtx::new(config, chapter);
        let mut ret = String::with_capacity(text.len());
        MATCHER.replace_all_with(text, &mut ret, |result, _, ret| {
            let pattern = Replacement::from_index(result.pattern().as_usize());
            pattern.write_to(ret, replacement_ctx);
            true
        });
        ret
    }
}

impl Preprocessor for DiataxisPreprocessor {
    fn name(&self) -> &str {
        "diataxis"
    }

    fn supports_renderer(&self, renderer: &str) -> bool {
        renderer == "html"
    }

    fn run(&self, ctx: &PreprocessorContext, mut book: Book) -> Result<Book> {
        let config = ctx
            .config
            .get_preprocessor(self.name())
            .map(Config::new)
            .unwrap_or_default();

        for section in &mut book.sections {
            self.preprocess_bookitem(section, &config)?;
        }

        Ok(book)
    }
}

#[derive(Default)]
struct Config;

impl Config {
    fn new(_raw: &Table) -> Self {
        Self
    }
}

#[derive(Copy, Clone)]
enum Replacement {
    Compass,
    Toc,
}

impl Replacement {
    const fn patterns() -> [&'static str; 2] {
        [Self::Compass.pattern(), Self::Toc.pattern()]
    }

    const fn pattern(&self) -> &'static str {
        match self {
            Self::Compass => "{{#diataxis compass}}",
            Self::Toc => "{{#diataxis toc}}",
        }
    }

    fn from_index(index: usize) -> Self {
        [Self::Compass, Self::Toc][index]
    }

    const COMPASS: &str = indoc! {r#"
        <div class="quote-grid">
            <blockquote>
                    <p>
                        <div class="diataxis-card-header"><a href="./tutorials/index.html">Tutorials</a></div>
                        Hands-on lessons in operating freight
                    </p>
                </a>
            </blockquote>
            <blockquote>
                <p>
                    <div class="diataxis-card-header">
                        <a href="./how-to/index.html">How-to guides</a>
                    </div>
                    Step-by-step instructions for common tasks
                </p>
            </blockquote>
            <blockquote>
                <p>
                    <div class="diataxis-card-header">
                        <a href="./reference-materials/index.html">Reference materials</a>
                    </div>
                    Technical information about freight
                </p>
            </blockquote>
            <blockquote>
                <p>
                    <div class="diataxis-card-header">
                        <a href="./explanations/index.html">Explanations</a>
                    </div>
                    Long-form discussion of key topics
                </p>
            </blockquote>
        </div>
    "#};

    fn write_to(&self, buf: &mut String, config: &Config) {
        match self {
            Self::Compass => buf.push_str(Self::COMPASS),
            Self::Toc => self.write_toc_to(buf, config),
        };
    }

    fn write_toc_to(&self, buf: &mut String, _config: &Config) {
        buf.push_str("TOC HERE");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // use googletest::expect_that;
    use indoc::indoc;
    use mdbook::preprocess::CmdPreprocessor;

    #[test]
    fn support() {
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
                        "content": "# Chapter 1\nasdf {{#diataxis toc}} fdsa",
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
        panic!("{book:?}");
    }
}
