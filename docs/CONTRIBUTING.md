# Contributing to PDF-CLI

Thank you for your interest in contributing to PDF-CLI! This document provides guidelines for contributors.

## Code of Conduct

This project adheres to a code of conduct. By participating, you are expected to uphold this code.

## Getting Started

### Prerequisites

- Rust (latest stable version)
- Git

### Setup

1. Fork the repository
2. Clone your fork:
   ```bash
   git clone https://github.com/yourusername/pdf-cli.git
   cd pdf-cli
   ```
3. Create a new branch for your feature:
   ```bash
   git checkout -b feature-name
   ```

## Development Workflow

### Building

```bash
cargo build
```

### Running Tests

```bash
cargo test
```

### Code Style

This project uses standard Rust formatting:

```bash
cargo fmt
```

### Linting

```bash
cargo clippy
```

## Contributing Guidelines

### Types of Contributions

We welcome the following types of contributions:

1. **Bug Reports**: File issues for bugs you encounter
2. **Feature Requests**: Suggest new features
3. **Pull Requests**: Submit code improvements
4. **Documentation**: Improve documentation
5. **Tests**: Add or improve tests

### Submitting Changes

1. Ensure your code follows the project's style
2. Add tests for new functionality
3. Update documentation as needed
4. Ensure all tests pass
5. Commit your changes with descriptive messages
6. Push to your fork
7. Create a pull request

### Commit Message Format

Use conventional commit messages:

- `feat:` for new features
- `fix:` for bug fixes
- `docs:` for documentation changes
- `style:` for code style changes
- `refactor:` for code refactoring
- `test:` for adding tests
- `chore:` for maintenance tasks

Example:

```
feat: add PDF encryption support

- Implement password protection for PDFs
- Add user/owner permission controls
- Update CLI interface with encryption options
- Add tests for encryption functionality
```

### Code Review Process

1. All submissions require review
2. Maintainers may request changes
3. Address feedback promptly
4. Maintain a respectful, constructive tone

## Architecture

The project is organized into several modules:

- `pdf/`: PDF parsing and text extraction
- `pdf_generator/`: PDF creation from scratch
- `markdown/`: Markdown parsing and conversion
- `image/`: Image handling for PDFs
- `compression/`: Stream compression utilities

When adding features, consider which module they belong to or if a new module is needed.

## Testing

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name

# Run with output
cargo test -- --nocapture
```

### Adding Tests

1. Add unit tests to `src/lib.rs`
2. Test both success and failure cases
3. Use descriptive test names
4. Keep tests focused and isolated

## Documentation

### Code Documentation

Add Rust doc comments to public functions:

```rust
/// Converts a PDF file to Markdown format
///
/// # Arguments
///
/// * `input_file` - Path to the input PDF file
/// * `output_file` - Path to the output Markdown file
///
/// # Returns
///
/// `Ok(())` if successful, `Err(e)` otherwise
pub fn pdf_to_markdown(input_file: &str, output_file: &str) -> Result<()> {
    // implementation
}
```

### User Documentation

Update relevant documentation files:

- README.md for user-facing features
- SPEC.md for technical specifications
- ARCHITECTURE.md for architectural decisions

## Performance Considerations

- Profile code changes with `cargo bench`
- Consider memory usage for large files
- Optimize for common use cases
- Document performance characteristics

## Security

- Follow secure coding practices
- Validate input data
- Handle errors gracefully
- Don't expose sensitive data in logs

## Release Process

Releases are versioned using semantic versioning:

1. **Major**: Breaking changes
2. **Minor**: New features (backward compatible)
3. **Patch**: Bug fixes (backward compatible)

## Getting Help

- Check existing issues before creating new ones
- Use the `discussions` tab for questions
- Join our community discussions

## License

By contributing, you agree that your contributions will be licensed under the project's license.
