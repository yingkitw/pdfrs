//! pdfcli command-line interface: argument parsing, dispatch, shared helpers.

mod args;
mod commands;

use args::{Cli, Commands};
use clap::Parser;
use pdfrs::{i18n, optimization, plugin};

fn resolve_locale(cli_lang: &Option<String>) -> i18n::Locale {
    cli_lang
        .as_deref()
        .and_then(i18n::Locale::parse)
        .unwrap_or_else(i18n::Locale::from_env)
}

fn build_plugin_registry(plugins: &str) -> plugin::PluginRegistry {
    let mut registry = plugin::PluginRegistry::new();
    for name in plugins.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        match name.to_ascii_lowercase().as_str() {
            "callouts" | "callout" => {
                registry.register_parser(plugin::CalloutPlugin);
                registry.register_generator(plugin::CalloutPlugin);
            }
            other => eprintln!("Warning: unknown plugin '{}'", other),
        }
    }
    registry
}

fn parse_optimization_profile(s: &str) -> optimization::OptimizationProfile {
    match s.to_lowercase().as_str() {
        "web" => optimization::OptimizationProfile::web(),
        "print" => optimization::OptimizationProfile::print(),
        "archive" => optimization::OptimizationProfile::archive(),
        "ebook" => optimization::OptimizationProfile::ebook(),
        _ => optimization::OptimizationProfile::archive(),
    }
}

pub(crate) fn run() {
    let cli = Cli::parse();
    let locale = resolve_locale(&cli.lang);

    match cli.command {
        Commands::GenerateComprehensive {
            output,
            landscape,
            linearize,
            font_size,
            columns,
        } => commands::generation::cmd_generate_comprehensive(
            output, landscape, linearize, font_size, columns,
        ),
        Commands::PdfToMd { input, output } => commands::conversion::cmd_pdf_to_md(input, output),
        Commands::MdToPdf {
            input,
            output,
            font,
            font_size,
            landscape,
            rtl,
            columns,
            plugins,
            profile,
        } => commands::generation::cmd_md_to_pdf(
            input, output, font, font_size, landscape, rtl, columns, plugins, profile,
        ),
        Commands::HtmlToPdf {
            input,
            output,
            font,
            font_size,
            landscape,
            rtl,
            columns,
            profile,
        } => commands::generation::cmd_html_to_pdf(
            input, output, font, font_size, landscape, rtl, columns, profile,
        ),
        Commands::Extract { input } => commands::conversion::cmd_extract(input),
        Commands::Create {
            output,
            text,
            font,
            font_size,
            landscape,
            profile,
        } => commands::generation::cmd_create(output, text, font, font_size, landscape, profile),
        Commands::CreateStreaming {
            output,
            text,
            landscape,
        } => commands::generation::cmd_create_streaming(output, text, landscape),
        Commands::AddImage {
            pdf_file,
            image_file,
            x,
            y,
            width,
            height,
        } => commands::manipulate::cmd_add_image(pdf_file, image_file, x, y, width, height),
        Commands::FilterImage {
            input,
            output,
            filters,
            width,
            height,
        } => commands::manipulate::cmd_filter_image(input, output, filters, width, height),
        Commands::Merge { inputs, output } => commands::manipulate::cmd_merge(inputs, output),
        Commands::Split {
            input,
            output,
            start,
            end,
        } => commands::manipulate::cmd_split(input, output, start, end),
        Commands::Watermark {
            input,
            output,
            text,
            size,
            opacity,
        } => commands::manipulate::cmd_watermark(input, output, text, size, opacity),
        Commands::Reorder {
            input,
            output,
            pages,
        } => commands::manipulate::cmd_reorder(input, output, pages),
        Commands::Rotate {
            input,
            output,
            angle,
        } => commands::manipulate::cmd_rotate(input, output, angle),
        Commands::MdToPdfMeta {
            input,
            output,
            title,
            author,
            subject,
            keywords,
            custom,
            font,
            font_size,
            landscape,
        } => commands::generation::cmd_md_to_pdf_meta(
            input, output, title, author, subject, keywords, custom, font, font_size, landscape,
        ),
        Commands::CreateForm {
            output,
            text,
            fields,
            font,
            font_size,
        } => commands::generation::cmd_create_form(output, text, fields, font, font_size),
        Commands::DetectFormFields { input } => commands::conversion::cmd_detect_form_fields(input),
        Commands::FillFormFields {
            input,
            output,
            values,
        } => commands::conversion::cmd_fill_form_fields(input, output, values),
        Commands::DetectStructure { input } => commands::conversion::cmd_detect_structure(input),
        Commands::OptimizePdf {
            input,
            output,
            profile,
        } => commands::manipulate::cmd_optimize_pdf(input, output, profile),
        Commands::LinearizePdf { input, output } => {
            commands::manipulate::cmd_linearize_pdf(input, output)
        }
        Commands::IncrementalUpdate {
            input,
            output,
            title,
            author,
            note,
        } => commands::manipulate::cmd_incremental_update(input, output, title, author, note),
        Commands::OverlayImage {
            input,
            output,
            image,
            x,
            y,
            width,
            height,
            opacity,
        } => commands::manipulate::cmd_overlay_image(
            input, output, image, x, y, width, height, opacity,
        ),
        Commands::WatermarkAdvanced {
            input,
            output,
            text,
            image,
            opacity,
            position,
        } => commands::manipulate::cmd_watermark_advanced(
            input, output, text, image, opacity, position,
        ),
        Commands::ExtractTables { input, output } => {
            commands::conversion::cmd_extract_tables(input, output)
        }
        Commands::ExtractImages { input, output } => {
            commands::conversion::cmd_extract_images(input, output)
        }
        Commands::Sign {
            input,
            output,
            signer,
            reason,
            location,
            contact,
            certificate,
            cert_id,
            cert_store,
        } => commands::security::cmd_sign(
            input,
            output,
            signer,
            reason,
            location,
            contact,
            certificate,
            cert_id,
            cert_store,
        ),
        Commands::ImportCertificate {
            id,
            file,
            subject,
            store,
        } => commands::security::cmd_import_certificate(id, file, subject, store),
        Commands::ListCertificates { store } => commands::security::cmd_list_certificates(store),
        Commands::VerifySignature { input } => commands::security::cmd_verify_signature(input),
        Commands::Protect {
            input,
            output,
            user_password,
            owner_password,
            algorithm,
            allow_print,
            allow_copy,
            allow_modify,
            allow_annotate,
            allow_fill_forms,
            allow_extract,
            allow_assemble,
            allow_print_high_quality,
            read_only,
        } => commands::security::cmd_protect(
            input,
            output,
            user_password,
            owner_password,
            algorithm,
            allow_print,
            allow_copy,
            allow_modify,
            allow_annotate,
            allow_fill_forms,
            allow_extract,
            allow_assemble,
            allow_print_high_quality,
            read_only,
        ),
        Commands::Validate { input } => commands::inspect::cmd_validate(locale, input),
        Commands::ValidatePdfa { input } => commands::inspect::cmd_validate_pdfa(input),
        Commands::ValidatePdfa3 { input } => commands::inspect::cmd_validate_pdfa3(input),
        Commands::ValidatePdfua { input } => commands::inspect::cmd_validate_pdfua(input),
        Commands::CheckScreenReader { input } => commands::inspect::cmd_check_screen_reader(input),
        Commands::DiffPdfs { old, new } => commands::inspect::cmd_diff_pdfs(old, new),
        Commands::SanitizePdf { input, output } => {
            commands::security::cmd_sanitize_pdf(input, output)
        }
        Commands::SandboxPdf { input, output } => {
            commands::security::cmd_sandbox_pdf(input, output)
        }
        Commands::WatchMarkdown {
            input,
            output,
            font,
            font_size,
            orientation,
            interval,
        } => commands::generation::cmd_watch_markdown(
            input,
            output,
            font,
            font_size,
            orientation,
            interval,
        ),
        Commands::Repl => commands::service::cmd_repl(),
        Commands::CreatePortfolio {
            output,
            files,
            title,
        } => commands::manipulate::cmd_create_portfolio(output, files, title),
        Commands::DrawVector { output, landscape } => {
            commands::vector_raster::cmd_draw_vector(output, landscape)
        }
        Commands::DrawSvg {
            output,
            path,
            file,
            landscape,
            line_width,
            fill,
        } => commands::vector_raster::cmd_draw_svg(output, path, file, landscape, line_width, fill),
        Commands::AttachFile {
            input,
            output,
            file,
            name,
        } => commands::manipulate::cmd_attach_file(input, output, file, name),
        Commands::Embed3d {
            output,
            model,
            label,
            x,
            y,
            width,
            height,
            activate_on_open,
        } => commands::manipulate::cmd_embed3d(
            output,
            model,
            label,
            x,
            y,
            width,
            height,
            activate_on_open,
        ),
        Commands::RasterizePdf {
            input,
            output,
            page,
            dpi,
        } => commands::vector_raster::cmd_rasterize_pdf(input, output, page, dpi),
        Commands::SearchPdf {
            input,
            query,
            case_insensitive,
            json,
        } => commands::inspect::cmd_search_pdf(input, query, case_insensitive, json),
        Commands::RedactPdf {
            input,
            output,
            region,
            strip,
        } => commands::inspect::cmd_redact_pdf(input, output, region, strip),
        Commands::DrawSvgFile {
            input,
            output,
            landscape,
        } => commands::vector_raster::cmd_draw_svg_file(input, output, landscape),
    }
}
