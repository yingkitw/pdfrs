// Performance benchmarks for PDF operations
//
// Run benchmarks with: cargo bench

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use pdf_rs::{pdf, pdf_ops, pdf_generator, elements, builder, optimization, parallel};
use std::collections::HashMap;

/// Benchmark markdown parsing
fn bench_markdown_parsing(c: &mut Criterion) {
    let small_md = "# Title\n\nSome text here.";
    let medium_md = "# Title\n\n## Section 1\n\nText here.\n\n## Section 2\n\nMore text.";
    // Create a large markdown document for testing
    let large_md: String = (0..100).map(|i| format!("## Section {}\n\nContent for section {}.\n\n", i, i)).collect();

    let mut group = c.benchmark_group("markdown_parsing");

    group.bench_function("small", |b| {
        b.iter(|| elements::parse_markdown(black_box(small_md)))
    });

    group.bench_function("medium", |b| {
        b.iter(|| elements::parse_markdown(black_box(medium_md)))
    });

    group.bench_function("large", |b| {
        b.iter(|| elements::parse_markdown(black_box(large_md.as_str())))
    });

    group.finish();
}

/// Benchmark PDF generation from elements
fn bench_pdf_generation(c: &mut Criterion) {
    let md_content = "# Test Document\n\nThis is a test document with some content.\n\n## Section 1\n\nContent here.";
    let elements = elements::parse_markdown(md_content);

    let mut group = c.benchmark_group("pdf_generation");

    group.bench_function("from_elements", |b| {
        b.iter(|| {
            let layout = pdf_generator::PageLayout::portrait();
            pdf_generator::create_pdf_from_elements_with_layout(
                black_box("/tmp/bench_test.pdf"),
                black_box(&elements),
                "Helvetica",
                12.0,
                layout,
            )
        })
    });

    group.finish();
}

/// Benchmark PDF text extraction
fn bench_pdf_extraction(c: &mut Criterion) {
    let mut group = c.benchmark_group("pdf_extraction");

    // Create a test PDF first
    let test_pdf = "/tmp/bench_extraction_test.pdf";
    let md_content = "# Test\n\nContent for extraction benchmark.";
    let elements = elements::parse_markdown(md_content);
    let _ = pdf_generator::create_pdf_from_elements_with_layout(
        test_pdf,
        &elements,
        "Helvetica",
        12.0,
        pdf_generator::PageLayout::portrait(),
    );

    group.bench_function("extract_text", |b| {
        b.iter(|| pdf::extract_text(black_box(test_pdf)))
    });

    group.finish();
}

/// Benchmark PDF metadata operations
fn bench_metadata_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("metadata");

    group.bench_function("create_metadata", |b| {
        b.iter(|| {
            let mut meta = pdf_ops::PdfMetadata::new();
            meta.title = Some("Test Title".to_string());
            meta.author = Some("Test Author".to_string());
            meta.add_custom_field("Key1".to_string(), "Value1".to_string());
            meta.add_custom_field("Key2".to_string(), "Value2".to_string());
            black_box(meta)
        })
    });

    group.finish();
}

/// Benchmark image operations
fn bench_image_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("image_operations");

    group.bench_function("scale_dimensions", |b| {
        b.iter(|| pdf_rs::image::scale_to_fit(black_box(1920), black_box(1080), 800.0, 600.0))
    });

    group.finish();
}

/// Benchmark compression/decompression
fn bench_compression(c: &mut Criterion) {
    let test_data = vec![42u8; 10_000]; // 10KB of test data

    let mut group = c.benchmark_group("compression");

    group.bench_function("compress_10kb", |b| {
        b.iter(|| pdf_rs::compression::compress_deflate(black_box(&test_data)))
    });

    group.bench_function("decompress_10kb", |b| {
        let compressed = pdf_rs::compression::compress_deflate(&test_data).unwrap();
        b.iter(|| pdf_rs::compression::decompress_deflate(black_box(&compressed)))
    });

    group.finish();
}

/// Benchmark PDF operations at different scales
fn bench_pdf_scalability(c: &mut Criterion) {
    let mut group = c.benchmark_group("scalability");

    for page_count in [1, 5, 10, 20].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(page_count), page_count, |b, &pages| {
            b.iter(|| {
                let mut generator = pdf_generator::PdfGenerator::new();
                for _ in 0..pages {
                    let content = b"BT /F1 12 Tf 72 720 Td (Test) Tj ET\n";
                    let _ = generator.add_stream_object("<< /Length 29 >>\n".to_string(), content.to_vec());
                }
                black_box(generator.generate())
            })
        });
    }

    group.finish();
}

/// Benchmark Builder API vs direct API
fn bench_builder_api(c: &mut Criterion) {
    let mut group = c.benchmark_group("builder_api");

    // Direct API benchmark
    group.bench_function("direct_api", |b| {
        b.iter(|| {
            let elements = vec![
                elements::Element::Heading { text: "Title".to_string(), level: 1 },
                elements::Element::Paragraph { text: "Content".to_string() },
            ];
            pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, pdf_generator::PageLayout::portrait())
        })
    });

    // Builder API benchmark
    group.bench_function("builder_api", |b| {
        b.iter(|| {
            builder::PdfBuilder::new()
                .add_heading("Title", 1)
                .add_paragraph("Content")
                .build_bytes()
        })
    });

    group.finish();
}

/// Benchmark parallel vs sequential PDF generation
fn bench_parallel_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel_generation");

    // Create test markdown content
    let inputs: HashMap<String, String> = (0..10)
        .map(|i| (format!("doc{}.md", i), format!("# Document {}\n\nContent {}", i, i)))
        .collect();

    group.bench_function("sequential", |b| {
        b.iter(|| {
            let mut results = HashMap::new();
            for (filename, markdown) in &inputs {
                let elements = elements::parse_markdown(markdown);
                let pdf_bytes = pdf_generator::generate_pdf_bytes(
                    &elements,
                    "Helvetica",
                    12.0,
                    pdf_generator::PageLayout::portrait(),
                ).unwrap();
                results.insert(filename.clone(), pdf_bytes);
            }
            black_box(results)
        })
    });

    group.bench_function("parallel", |b| {
        b.iter(|| {
            let generator = parallel::ParallelPdfGenerator::new();
            black_box(generator.generate_markdown_pdfs_parallel(&inputs))
        })
    });

    group.finish();
}

/// Benchmark streaming PDF generation
fn bench_streaming_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("streaming_generation");

    let large_elements: Vec<elements::Element> = (0..100)
        .map(|i| elements::Element::Heading {
            text: format!("Section {}", i),
            level: 2,
        })
        .chain((0..100).map(|i| elements::Element::Paragraph {
            text: format!("Content for section {}", i),
        }))
        .collect();

    group.bench_function("streaming_small", |b| {
        b.iter(|| {
            let small_elements: Vec<_> = large_elements.iter().take(10).cloned().collect();
            let mut generator = pdf_rs::streaming::StreamingPdfGenerator::new(
                "/tmp/bench_stream_small.pdf",
                pdf_generator::PageLayout::portrait(),
            ).unwrap();
            for elem in &small_elements {
                let _ = generator.add_element(elem.clone());
            }
            black_box(generator.finish())
        })
    });

    group.bench_function("streaming_large", |b| {
        b.iter(|| {
            let mut generator = pdf_rs::streaming::StreamingPdfGenerator::new(
                "/tmp/bench_stream_large.pdf",
                pdf_generator::PageLayout::portrait(),
            ).unwrap();
            for elem in &large_elements {
                let _ = generator.add_element(elem.clone());
            }
            black_box(generator.finish())
        })
    });

    group.finish();
}

/// Benchmark optimization profiles
fn bench_optimization_profiles(c: &mut Criterion) {
    let mut group = c.benchmark_group("optimization_profiles");

    let elements = vec![
        elements::Element::Heading { text: "Test".to_string(), level: 1 },
        elements::Element::Paragraph { text: "Content".to_string() },
    ];

    for profile in [
        optimization::OptimizationProfile::Web,
        optimization::OptimizationProfile::Print,
        optimization::OptimizationProfile::Archive,
        optimization::OptimizationProfile::Ebook,
    ].iter() {
        group.bench_with_input(
            BenchmarkId::new("generate", format!("{:?}", profile)),
            profile,
            |b, profile| {
                b.iter(|| {
                    let generator = optimization::OptimizedPdfGenerator::new(*profile);
                    black_box(generator.generate_bytes(&elements))
                })
            },
        );
    }

    group.finish();
}

/// Benchmark merge operations
fn bench_merge_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("merge_operations");

    // Create test PDFs first
    let pdf_paths: Vec<String> = (0..5).map(|i| {
        let path = format!("/tmp/bench_merge_{}.pdf", i);
        let elements = vec![
            elements::Element::Heading { text: format!("PDF {}", i), level: 1 },
            elements::Element::Paragraph { text: format!("Content {}", i) },
        ];
        let _ = pdf_generator::create_pdf_from_elements_with_layout(
            &path,
            &elements,
            "Helvetica",
            12.0,
            pdf_generator::PageLayout::portrait(),
        );
        path
    }).collect();

    let pdf_paths_str: Vec<&str> = pdf_paths.iter().map(|s| s.as_str()).collect();

    group.bench_function("merge_5_pdfs", |b| {
        b.iter(|| {
            pdf_ops::merge_pdfs(black_box(&pdf_paths_str), black_box("/tmp/bench_merge_output.pdf"))
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_markdown_parsing,
    bench_pdf_generation,
    bench_pdf_extraction,
    bench_metadata_operations,
    bench_image_operations,
    bench_compression,
    bench_pdf_scalability,
    bench_builder_api,
    bench_parallel_generation,
    bench_streaming_generation,
    bench_optimization_profiles,
    bench_merge_operations
);

criterion_main!(benches);
