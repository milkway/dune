SEXP savvy_ffi_measure__ffi(SEXP c_arg__plot, SEXP c_arg__scale, SEXP c_arg__names, SEXP c_arg__specs);
SEXP savvy_ffi_option_names__ffi(void);
SEXP savvy_ffi_pss_deck__ffi(SEXP c_arg__deck, SEXP c_arg__is_path, SEXP c_arg__include_names, SEXP c_arg__include_texts, SEXP c_arg__opt_names, SEXP c_arg__opt_values, SEXP c_arg__tmax_divisor, SEXP c_arg__freq, SEXP c_arg__tstab, SEXP c_arg__osc_node, SEXP c_arg__points, SEXP c_arg__harmonics, SEXP c_arg__max_iter, SEXP c_arg__steady_coeff, SEXP c_arg__uic);
SEXP savvy_ffi_read_raw__ffi(SEXP c_arg__path);
SEXP savvy_ffi_run_deck__ffi(SEXP c_arg__deck, SEXP c_arg__is_path, SEXP c_arg__include_names, SEXP c_arg__include_texts, SEXP c_arg__opt_names, SEXP c_arg__opt_values, SEXP c_arg__tmax_divisor);
SEXP savvy_ffi_sweep_deck__ffi(SEXP c_arg__deck, SEXP c_arg__is_path, SEXP c_arg__include_names, SEXP c_arg__include_texts, SEXP c_arg__opt_names, SEXP c_arg__opt_values, SEXP c_arg__tmax_divisor, SEXP c_arg__params, SEXP c_arg__values, SEXP c_arg__threads);
SEXP savvy_ffi_write_raw__ffi(SEXP c_arg__path, SEXP c_arg__plots, SEXP c_arg__binary);

// methods and associated functions for SpiceSession
SEXP savvy_SpiceSession_alter__ffi(SEXP self__, SEXP c_arg__refdes, SEXP c_arg__param, SEXP c_arg__value, SEXP c_arg__is_list);
SEXP savvy_SpiceSession_alterable__ffi(SEXP self__, SEXP c_arg__refdes);
SEXP savvy_SpiceSession_open__ffi(SEXP c_arg__deck, SEXP c_arg__is_path, SEXP c_arg__include_names, SEXP c_arg__include_texts, SEXP c_arg__opt_names, SEXP c_arg__opt_values, SEXP c_arg__tmax_divisor);
SEXP savvy_SpiceSession_param__ffi(SEXP self__, SEXP c_arg__refdes, SEXP c_arg__param);
SEXP savvy_SpiceSession_run__ffi(SEXP self__);
SEXP savvy_SpiceSession_set_temp__ffi(SEXP self__, SEXP c_arg__celsius);
SEXP savvy_SpiceSession_set_tmax_divisor__ffi(SEXP self__, SEXP c_arg__d);
SEXP savvy_SpiceSession_set_warm_start__ffi(SEXP self__, SEXP c_arg__on);
SEXP savvy_SpiceSession_status__ffi(SEXP self__);
SEXP savvy_SpiceSession_temp__ffi(SEXP self__);
SEXP savvy_SpiceSession_title__ffi(SEXP self__);
SEXP savvy_SpiceSession_tmax_divisor__ffi(SEXP self__);
SEXP savvy_SpiceSession_warm_start__ffi(SEXP self__);
SEXP savvy_SpiceSession_warm_stats__ffi(SEXP self__);
