extern crate glob;
extern crate lazy_static;
extern crate pretty_env_logger;

use glob::glob;
use lazy_static::lazy_static;
use log::{debug, error, info, warn};
use pulldown_cmark::{CodeBlockKind::Fenced, CowStr::Borrowed, Event, Parser, Tag::CodeBlock};
use regex::{Regex, RegexBuilder};
use std::collections::HashMap;
use std::path::Path;
use std::{env, fs, io};

enum CodeMacroParseError {
    MissingIndentifier,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct CodeMacro {
    name: String,
    content: String,
}

impl TryFrom<String> for CodeMacro {
    type Error = CodeMacroParseError;
    fn try_from(text: String) -> Result<Self, Self::Error> {
        lazy_static! {
            static ref MACRO_IDENT_RE: Regex = Regex::new(r"^//\s*(<<(.+)>>=)\s*\n(.*)").unwrap();
        }
        let captures = MACRO_IDENT_RE
            .captures(&text)
            .ok_or(CodeMacroParseError::MissingIndentifier)?;

        let definition = captures
            .get(1)
            .ok_or(CodeMacroParseError::MissingIndentifier)?
            .as_str();

        let name = captures
            .get(2)
            .ok_or(CodeMacroParseError::MissingIndentifier)?
            .as_str();

        Ok(CodeMacro {
            name: name.to_owned(),
            content: text.replace(definition, name),
        })
    }
}

type CodeMacroCollection = HashMap<String, CodeMacro>;

fn prepend_indents(text: &str, indents: usize) -> String {
    text.lines()
        .map(|x| format!("{}{}\n", "    ".repeat(indents), x))
        .collect()
}

fn expand_code_macros(code_macros: &CodeMacroCollection) -> String {
    let mut output = code_macros
        .get("*")
        .expect("No root macro found")
        .content
        .clone();

    let macro_re = RegexBuilder::new(r"^( *)//\s*<<(.+)>>\n")
        .multi_line(true)
        .build()
        .unwrap();

    while let Some(captures) = macro_re.captures(output.as_str()) {
        let indents: usize = captures.get(1).unwrap().as_str().len() / 4;
        let macro_name = captures.get(2).unwrap().as_str();
        let replacement = prepend_indents(
            code_macros
                .get(macro_name)
                .expect("A macro was used, but not defined.")
                .content
                .as_str(),
            indents,
        );
        debug!("Expanding macro {macro_name}");
        output = macro_re.replace(output.as_str(), replacement).into_owned();
    }
    output
}

fn tangle(path: &Path) -> Result<(), io::Error> {
    let input_file_contents = std::fs::read_to_string(path)?;
    let parser = Parser::new(&input_file_contents);
    let mut in_rust_code_block = false;
    let mut code_macros = CodeMacroCollection::new();

    for event in parser {
        match event {
            Event::Start(CodeBlock(Fenced(Borrowed("rust")))) => {
                in_rust_code_block = true;
            }
            Event::Text(text) => {
                if !in_rust_code_block {
                    continue;
                }
                if let Ok(new_macro) = CodeMacro::try_from(text.into_string()) {
                    if code_macros.contains_key(&new_macro.name) {
                        warn!("Redefinition found for macro {}", new_macro.name);
                    } else {
                        code_macros.insert(new_macro.name.clone(), new_macro);
                    }
                }
            }
            Event::End(CodeBlock(Fenced(Borrowed("rust")))) => {
                in_rust_code_block = false;
            }
            _ => (),
        }
    }

    let output_path_name = format!(
        "{}/{}.rs",
        path.parent().unwrap().to_str().unwrap(),
        path.file_stem().unwrap().to_str().unwrap()
    );

    let output_path = Path::new(&output_path_name);

    fs::write(output_path, expand_code_macros(&code_macros).as_str())?;

    info!(
        "Writing output of {} to {output_path_name}",
        path.to_str().unwrap()
    );

    Ok(())
}

fn main() -> Result<(), io::Error> {
    pretty_env_logger::init();
    let project_dir = env::args().nth(1).unwrap_or(".".to_string());
    let md_glob = format!("{project_dir}/src/**/*.md");

    for entry in glob(&md_glob).expect("Failed to read glob pattern") {
        match entry {
            Ok(path) => {
                info!("Tangling {}", path.display());
                tangle(&path)?;
            }
            Err(e) => error!("{e}"),
        }
    }

    Ok(())
}
