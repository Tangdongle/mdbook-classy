use clap::{Arg, ArgMatches, Command};
use mdbook::book::{Book, Chapter};
use mdbook::errors::Error;
use mdbook::preprocess::{CmdPreprocessor, Preprocessor, PreprocessorContext};
use mdbook::utils::new_cmark_parser;
use pulldown_cmark::{CowStr, Event, Parser, Tag};
use std::io;
use std::process;
use std::collections::VecDeque;

const MAX_DEPTH: usize = 254;

#[derive(Default)]
pub struct Blocky;

impl Blocky{
    pub fn new() -> Blocky {
        Blocky
    }
}

impl Preprocessor for Blocky {
    fn name(&self) -> &str {
        "blocky"
    }
    fn run(&self, _ctx: &PreprocessorContext, mut book: Book) -> Result<Book, Error> {
        book.for_each_mut(|book| {
            if let mdbook::BookItem::Chapter(chapter) = book {
                if let Err(e) = blocky(chapter) {
                    eprintln!("blocky error: {:?}", e);
                }
            }
        });
        Ok(book)
    }
    fn supports_renderer(&self, renderer: &str) -> bool {
        renderer == "html"
    }
}

struct EventClassAnnotator<'a> {
    stack: VecDeque<Event<'a>>,
    depth: usize,
}

impl<'a> Iterator for EventClassAnnotator<'a> {
    type Item = Event<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.stack.pop_front()?;
        if let Event::Text(CowStr::Borrowed(text)) = current {
            let text_len = text.len();
            if text_len < 5 {
                return Some(current)
            }
            // If the last event was the opening of a text element
            if text.starts_with("{:.") && text.ends_with("}") {
                if self.depth > 253 {
                    panic!("Error with recursion depth!: {}", text);
                }
                self.depth += 1;
                let mut class = text[3..text_len - 1].to_string();
                class.push_str(" blocky-block");
                if self.depth > 1 {
                    class.push_str(&format!(" block-level-{}", self.depth - 1));
                }
                let open_div = Event::Html(CowStr::from(format!("<div class=\"{}\">", class)));
                return Some(open_div)
            } else if text.starts_with("{:/.") && text.ends_with("}") {
                let close_div = Event::Html(CowStr::from("</div>"));
                if self.depth == 0 {
                    // Bad formatting
                    panic!("Bad formatting!: {}", text);
                }
                self.depth -= 1;
                return Some(close_div)
            } else {
                Some(current)
            }
        } else {
            Some(current)
        }
    }

}

impl<'a> EventClassAnnotator<'a> {
    fn new(stack: Vec<Event<'a>>) -> Self {
        Self {
            stack: stack.into(),
            depth: 0
        }
    }
}

/// This is where the markdown transformation actually happens.
/// Take paragraphs beginning with `{:.class-name}` and give them special rendering.
/// Mutation: the payload here is that it edits chapter.content.
fn blocky(chapter: &mut Chapter) -> Result<(), Error> {
    let incoming_events: Vec<Event> = new_cmark_parser(&chapter.content, false).collect();
    let new_events: Vec<Event> = EventClassAnnotator::new(incoming_events).collect();

    let mut buf = String::with_capacity(chapter.content.len() + 128);
    pulldown_cmark_to_cmark::cmark(new_events.into_iter(), &mut buf)
        .expect("can re-render cmark");
    chapter.content = buf;
    Ok(())
}

/// Housekeeping:
/// 1. Check compatibility between preprocessor and mdbook
/// 2. deserialize, run the transformation, and reserialize.
fn handle_preprocessing(pre: &dyn Preprocessor) -> Result<(), Error> {
    let (ctx, book) = CmdPreprocessor::parse_input(io::stdin())?;

    if ctx.mdbook_version != mdbook::MDBOOK_VERSION {
        // We should probably use the `semver` crate to check compatibility
        // here...
        eprintln!(
            "Warning: The {} plugin was built against version {} of mdbook, \
             but we're being called from version {}",
            pre.name(),
            mdbook::MDBOOK_VERSION,
            ctx.mdbook_version
        );
    }

    let processed_book = pre.run(&ctx, book)?;
    serde_json::to_writer(io::stdout(), &processed_book)?;

    Ok(())
}

/// Check to see if we support the processor (blocky only supports html right now)
fn handle_supports(pre: &dyn Preprocessor, sub_args: &ArgMatches) -> ! {
    let renderer = sub_args.get_one::<String>("renderer").expect("Required argument");
    let supported = pre.supports_renderer(&renderer);

    if supported {
        process::exit(0);
    } else {
        process::exit(1);
    }
}

fn main() {
    // 1. Define command interface, requiring renderer to be specified.
    let matches = Command::new("blocky")
        .about("A mdbook preprocessor that lets you design block sections for collections of elements")
        .subcommand(
            Command::new("supports")
                .arg(Arg::new("renderer").required(true))
                .about("Check whether a renderer is supported by this preprocessor"),
        )
        .get_matches();

    // 2. Instantiate the preprocessor.
    let preprocessor = Blocky::new();

    if let Some(sub_args) = matches.subcommand_matches("supports") {
        handle_supports(&preprocessor, sub_args);
    } else if let Err(e) = handle_preprocessing(&preprocessor) {
        eprintln!("{}", e);
        process::exit(1);
    }
}
