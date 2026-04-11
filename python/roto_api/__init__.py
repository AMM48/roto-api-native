"""Public Python API for the ``roto-api-native`` package.

The published package is intentionally load-only: it opens a prepared local
dataset snapshot and performs lookups in the compiled Rust extension.
Downloading and preparing dump data is handled outside the package.
"""

from importlib import import_module
from importlib.metadata import PackageNotFoundError, version
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from ._native import RotoLookup as _RotoLookupType

try:
    __version__ = version("roto-api-native")
except PackageNotFoundError:
    __version__ = "0.2.1"


def _load_roto_lookup_class():
    """Return the native ``RotoLookup`` class from the compiled extension."""
    return import_module("._native", __name__).RotoLookup


def __getattr__(name):
    if name == "RotoLookup":
        return _load_roto_lookup_class()
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")


def load_lookup(data_dir):
    """Load the native lookup engine from an already prepared data directory."""
    return _load_roto_lookup_class().from_data_dir(str(data_dir))


def open_lookup(data_dir):
    """Alias for ``load_lookup`` for callers that prefer a simpler verb."""
    return load_lookup(data_dir)

__all__ = [
    "RotoLookup",
    "load_lookup",
    "open_lookup",
    "__version__",
]
