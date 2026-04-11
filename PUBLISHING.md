# Publishing Guide

## Before First Release

1. Create your own GitHub repository for this package.
2. Update package metadata in:
   - `pyproject.toml`
   - `Cargo.toml`
3. Set your own URLs, maintainer info, and repository links.
4. Create a PyPI project or prepare trusted publishing.

## Recommended Metadata To Own

Update these before publishing:

- project description
- homepage/repository/issues URLs
- version
- README wording if you are branding/forking it

The upstream `LICENSE` file should remain because it is the original license grant.

## Local Validation

From the repo root:

```bash
cargo check
python -m py_compile python/roto_api/__init__.py scripts/bootstrap_data.py scripts/query_ips_native.py
python -m pytest
maturin build --release
```

Optional upload sanity check:

```bash
python -m pip install --upgrade twine
python -m twine check target/wheels/*
```

## GitHub Actions

This repo includes:

- `.github/workflows/pkg.yml`
  Runs CI, builds wheels, and builds an sdist.
- `.github/workflows/publish.yml`
  Publishes to PyPI on `v*` tags.

## PyPI Trusted Publishing

Recommended path:

1. Push the repo to GitHub.
2. In PyPI, configure a trusted publisher for this repository.
3. Configure the `pypi` environment in GitHub if you want environment protection.
4. Push a version tag like `v0.2.2`.

The publish workflow will:
- build wheels
- build an sdist
- upload them to PyPI

## Manual Upload Alternative

If you do not want trusted publishing yet:

```bash
python -m pip install --upgrade twine
python -m twine upload target/wheels/*
```

You can also upload to TestPyPI first:

```bash
python -m twine upload --repository testpypi target/wheels/*
```

## Release Checklist

1. Bump version in `pyproject.toml` and `Cargo.toml`.
2. Run local validation.
3. Commit changes.
4. Tag the release, e.g. `v0.2.2`.
5. Push branch and tag.
6. Verify GitHub Actions artifacts.
7. Verify the package page on PyPI.
8. Verify a fresh install:

```bash
pip install roto-api-native
python -c "import roto_api; print(roto_api.__version__)"
```
