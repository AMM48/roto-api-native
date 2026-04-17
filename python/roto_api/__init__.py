"""Public Python API for the ``roto-api-native`` package.

The package exposes:

- ``ensure_data(...)`` to explicitly bootstrap or refresh a local snapshot
- ``load_lookup(...)`` to load an existing snapshot
- ``open_lookup(...)`` as a convenience wrapper that can bootstrap and load
"""

from importlib import import_module
from importlib.metadata import PackageNotFoundError, version

from .data import ensure_data

try:
    __version__ = version("roto-api-native")
except PackageNotFoundError:
    __version__ = "0.2.4"


def _load_roto_lookup_class():
    """Return the native ``RotoLookup`` class from the compiled extension."""
    return import_module("._native", __name__).RotoLookup


def __getattr__(name):
    if name == "RotoLookup":
        return _load_roto_lookup_class()
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")


def load_lookup(data_dir, include_delegated=False):
    """Load the native lookup engine from an already prepared data directory."""
    return _load_roto_lookup_class().from_data_dir(
        str(data_dir), include_delegated=include_delegated
    )


def open_lookup(
    data_dir,
    refresh=False,
    include_delegated=False,
    del_ext_sources=None,
    riswhois_sources=None,
):
    """Ensure snapshot data exists, then load the native lookup engine.

    ``open_lookup`` is the convenience entry point for callers who want one
    function that can optionally refresh the upstream snapshot and then open it.
    """
    data_dir = ensure_data(
        data_dir,
        refresh=refresh,
        include_delegated=include_delegated,
        del_ext_sources=del_ext_sources,
        riswhois_sources=riswhois_sources,
    )
    return load_lookup(data_dir, include_delegated=include_delegated)

__all__ = [
    "RotoLookup",
    "ensure_data",
    "load_lookup",
    "open_lookup",
    "__version__",
]
