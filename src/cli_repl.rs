//! Interactive PDF REPL (`pdfcli repl`).

use pdfrs::optimization;
use pdfrs::pdf;
use regex::Regex;

/// Interactive REPL for PDF manipulation.
///
/// Supports commands:
/// - `load <file>` — load a PDF into the session
/// - `save <file>` — save the current PDF
/// - `text` — extract text from the loaded PDF
/// - `pages` — count pages in the loaded PDF
/// - `validate` — validate structural integrity
/// - `validate-pdfa` — check PDF/A-1b compliance
/// - `optimize [web|print|archive|ebook]` — optimize the PDF
/// - `sanitize` — remove dangerous content
/// - `attach <file> [name]` — embed a file attachment
/// - `info` — show document info (objects, pages, catalog)
/// - `help` — show available commands
/// - `quit` / `exit` — leave the REPL
pub fn run_repl() {
    use std::io::{self, Write};

    let mut doc: Option<pdf::PdfDocument> = None;
    let page_re = Regex::new(r"/Type\s+/Page[^s]").unwrap();

    println!("pdfrs PDF REPL — type 'help' for commands, 'quit' to exit.");

    loop {
        print!("pdf> ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            break;
        }
        let line = input.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        let cmd = parts.first().copied().unwrap_or("");
        let args = &parts[1..];

        match cmd {
            "quit" | "exit" => {
                println!("Goodbye.");
                break;
            }
            "help" => {
                println!("Commands:");
                println!("  load <file>           Load a PDF file");
                println!("  save <file>           Save the current PDF");
                println!("  text                  Extract text from loaded PDF");
                println!("  pages                 Count pages");
                println!("  validate              Validate structural integrity");
                println!("  validate-pdfa         Check PDF/A-1b compliance");
                println!("  optimize [profile]    Optimize (web|print|archive|ebook)");
                println!("  sanitize              Remove JS, launch actions, etc.");
                println!("  attach <file> [name]  Embed a file attachment");
                println!("  info                  Show document info");
                println!("  help                  Show this message");
                println!("  quit / exit           Leave the REPL");
            }
            "load" => {
                if args.is_empty() {
                    println!("Usage: load <file>");
                    continue;
                }
                let path = args[0];
                match pdf::PdfDocument::load_from_file(path) {
                    Ok(loaded) => {
                        println!("Loaded {} ({} objects)", path, loaded.objects.len());
                        doc = Some(loaded);
                    }
                    Err(e) => println!("Error loading PDF: {}", e),
                }
            }
            "save" => {
                if args.is_empty() {
                    println!("Usage: save <file>");
                    continue;
                }
                let Some(ref d) = doc else {
                    println!("No PDF loaded. Use 'load <file>' first.");
                    continue;
                };
                let path = args[0];
                match std::fs::write(path, d.to_bytes()) {
                    Ok(_) => println!("Saved to {}", path),
                    Err(e) => println!("Error saving PDF: {}", e),
                }
            }
            "text" => {
                let Some(ref d) = doc else {
                    println!("No PDF loaded. Use 'load <file>' first.");
                    continue;
                };
                match d.get_text() {
                    Ok(t) => println!("{}", t),
                    Err(e) => println!("Error extracting text: {}", e),
                }
            }
            "pages" => {
                let Some(ref d) = doc else {
                    println!("No PDF loaded. Use 'load <file>' first.");
                    continue;
                };
                let bytes = d.to_bytes();
                let content = String::from_utf8_lossy(&bytes);
                let count = page_re.find_iter(&content).count();
                println!("Pages: {}", count);
            }
            "validate" => {
                let Some(ref d) = doc else {
                    println!("No PDF loaded. Use 'load <file>' first.");
                    continue;
                };
                let result = pdf::validate_pdf_bytes(&d.to_bytes());
                println!("Valid: {}", result.valid);
                if !result.errors.is_empty() {
                    println!("Errors:");
                    for e in &result.errors {
                        println!("  - {}", e);
                    }
                }
            }
            "validate-pdfa" => {
                let Some(ref d) = doc else {
                    println!("No PDF loaded. Use 'load <file>' first.");
                    continue;
                };
                let result = pdf::validate_pdf_a_bytes(&d.to_bytes());
                println!("PDF/A-1b Compliant: {}", result.compliant);
                if !result.errors.is_empty() {
                    println!("Errors:");
                    for e in &result.errors {
                        println!("  - {}", e);
                    }
                }
            }
            "optimize" => {
                let Some(ref mut d) = doc else {
                    println!("No PDF loaded. Use 'load <file>' first.");
                    continue;
                };
                let profile = args.first().copied().unwrap_or("web");
                let settings = match profile {
                    "web" => optimization::OptimizationProfile::web(),
                    "print" => optimization::OptimizationProfile::print(),
                    "archive" => optimization::OptimizationProfile::archive(),
                    "ebook" => optimization::OptimizationProfile::ebook(),
                    _ => optimization::OptimizationProfile::web(),
                }
                .settings();
                let bytes = d.to_bytes();
                match optimization::optimize_pdf_bytes(&bytes, settings) {
                    Ok(optimized) => match pdf::PdfDocument::load_from_bytes(&optimized) {
                        Ok(reloaded) => {
                            println!("Optimized ({} -> {} bytes)", bytes.len(), optimized.len());
                            *d = reloaded;
                        }
                        Err(e) => println!("Error reloading optimized PDF: {}", e),
                    },
                    Err(e) => println!("Error optimizing PDF: {}", e),
                }
            }
            "sanitize" => {
                let Some(ref mut d) = doc else {
                    println!("No PDF loaded. Use 'load <file>' first.");
                    continue;
                };
                d.sanitize();
                println!("Sanitized PDF (removed JS, launch actions, etc.)");
            }
            "attach" => {
                let Some(ref mut d) = doc else {
                    println!("No PDF loaded. Use 'load <file>' first.");
                    continue;
                };
                if args.is_empty() {
                    println!("Usage: attach <file> [name]");
                    continue;
                }
                let file_path = args[0];
                let name = args.get(1).copied().unwrap_or_else(|| {
                    std::path::Path::new(file_path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(file_path)
                });
                match std::fs::read(file_path) {
                    Ok(data) => match d.embed_file(name, &data) {
                        Ok(_) => println!("Attached '{}' as '{}'", file_path, name),
                        Err(e) => println!("Error embedding file: {}", e),
                    },
                    Err(e) => println!("Error reading file: {}", e),
                }
            }
            "info" => {
                let Some(ref d) = doc else {
                    println!("No PDF loaded. Use 'load <file>' first.");
                    continue;
                };
                let bytes = d.to_bytes();
                let content = String::from_utf8_lossy(&bytes);
                let pages = page_re.find_iter(&content).count();
                println!("Objects: {}", d.objects.len());
                println!("Pages: {}", pages);
                println!("Catalog ID: {}", d.catalog);
                println!("Version: {}", d.version);
                println!("Has embedded files: {}", content.contains("/EmbeddedFiles"));
            }
            _ => {
                println!(
                    "Unknown command '{}'. Type 'help' for available commands.",
                    cmd
                );
            }
        }
    }
}
