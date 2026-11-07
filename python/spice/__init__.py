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
and Monte Carlo, :class:`SweepResult`); :class:`Session` keeps a circuit alive
to alter values and re-run it (sliders, fitting loops); :func:`pss` runs the
periodic steady state of a driven circuit (:class:`PssResult`). Results are :class:`Result` (one run)
holding :class:`Plot` objects (one per analysis plot), each with NumPy
columns (:meth:`Plot.column`) and a polars frame (:meth:`Plot.df`). The deck's
``.measure`` cards come back in :attr:`Result.measurements`; :func:`measure`
measures any plot directly.
"""

from ._errors import Diagnostic, ErrorKind, PssConvergenceError, Severity, SpiceError
from ._measure import MeasureAnalysis, Measurement, measure
from ._pss import PSS_FREQ_PLOT, PSS_TIME_PLOT, PssResult, pss
from ._result import Column, Plot, Result, Variable
from ._session import AlterValue, Session, WarmStats
from ._simulate import OPTION_NAMES, OptionValue, simulate
from ._spice import __version__
from ._sweep import SweepMode, SweepResult, sweep

__all__ = [
    "OPTION_NAMES",
    "PSS_FREQ_PLOT",
    "PSS_TIME_PLOT",
    "AlterValue",
    "Column",
    "Diagnostic",
    "ErrorKind",
    "MeasureAnalysis",
    "Measurement",
    "OptionValue",
    "Plot",
    "PssConvergenceError",
    "PssResult",
    "Result",
    "Session",
    "Severity",
    "SpiceError",
    "SweepMode",
    "SweepResult",
    "Variable",
    "WarmStats",
    "__version__",
    "measure",
    "pss",
    "simulate",
    "sweep",
]
