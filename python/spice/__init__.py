"""SPICE3f5-faithful circuit simulation for Python, on the Dune engine.

>>> import spice
>>> res = spice.simulate(\"\"\"rc lowpass
... vin 1 0 dc 0 ac 1
... r1 1 2 1k
... c1 2 0 159.155n
... .ac dec 10 10 1meg
... .end
... \"\"\")
>>> ac = res.plot("AC Analysis")
>>> ac.df().columns[:3]
['frequency', 're:v(1)', 'im:v(1)']

The entry point is :func:`simulate`; results are :class:`Result` (one run)
holding :class:`Plot` objects (one per analysis plot), each with NumPy
columns (:meth:`Plot.column`) and a polars frame (:meth:`Plot.df`).
"""

from ._errors import Diagnostic, ErrorKind, Severity, SpiceError
from ._result import Column, Plot, Result, Variable
from ._simulate import OPTION_NAMES, OptionValue, simulate
from ._spice import __version__

__all__ = [
    "OPTION_NAMES",
    "Column",
    "Diagnostic",
    "ErrorKind",
    "OptionValue",
    "Plot",
    "Result",
    "Severity",
    "SpiceError",
    "Variable",
    "__version__",
    "simulate",
]
