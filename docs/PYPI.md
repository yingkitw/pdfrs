# Publishing to PyPI (`pip install pdfrs`)

Same codebase as [github.com/yingkitw/pdfrs](https://github.com/yingkitw/pdfrs); version tracks `Cargo.toml`.

The Python package uses [maturin](https://www.maturin.rs/) to build a
PyO3 extension (`pdfrs._pdfrs`) plus the thin `python/pdfrs` package.

## Prerequisites

- Rust toolchain (same as the crate)
- Python 3.8+
- `pip install maturin` (or `pipx install maturin`)

## Local development

Create a virtualenv (maturin requires one for `develop`):

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install maturin pytest
maturin develop --release
python -m pytest python/tests
```

Or install a built wheel: `pip install target/wheels/pdfrs-*.whl`

## Build wheels

```bash
./scripts/build-pypi.sh
# artifacts in dist/*.whl
```

## Publish

1. Register `__token__` at [pypi.org](https://pypi.org) (or TestPyPI first).
2. Align `version` in `Cargo.toml` with the release you intend to ship.
3. Run:

```bash
maturin publish --skip-existing
```

PyPI uses the **same version** as the Rust crate (`dynamic = ["version"]` in
`pyproject.toml` reads from `Cargo.toml`).

## API

```python
import pdfrs

pdf = pdfrs.render_markdown_to_pdf("# Title\n\nBody")
with open("out.pdf", "wb") as f:
    f.write(pdf)
```

See `python/pdfrs/__init__.py` for exported symbols.
