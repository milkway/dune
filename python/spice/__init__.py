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

The entry points are :func:`simulate` and :func:`sweep` (parametric sweeps
and Monte Carlo, :class:`SweepResult`); results are :class:`Result` (one run)
holding :class:`Plot` objects (one per analysis plot), each with NumPy
columns (:meth:`Plot.column`) and a polars frame (:meth:`Plot.df`). The deck's
``.measure`` cards come back in :attr:`Result.measurements`; :func:`measure`
measures any plot directly.
"""

from ._errors import Diagnostic, ErrorKind, Severity, SpiceError
from ._measure import MeasureAnalysis, Measurement, measure
from ._result import Column, Plot, Result, Variable
from ._simulate import OPTION_NAMES, OptionValue, simulate
from ._spice import __version__
from ._sweep import SweepMode, SweepResult, sweep

__all__ = [
    "OPTION_NAMES",
    "Column",
    "Diagnostic",
    "ErrorKind",
    "MeasureAnalysis",
    "Measurement",
    "OptionValue",
    "Plot",
    "Result",
    "Severity",
    "SpiceError",
    "SweepMode",
    "SweepResult",
    "Variable",
    "__version__",
    "measure",
    "simulate",
    "sweep",
]
