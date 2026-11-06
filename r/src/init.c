
// clang-format sorts includes unless SortIncludes: Never. However, the ordering
// does matter here. So, we need to disable clang-format for safety.

// clang-format off
#include <stdint.h>
#include <Rinternals.h>
#include <R_ext/Parse.h>
// clang-format on

#include "rust/api.h"

static uintptr_t TAGGED_POINTER_MASK = (uintptr_t)1;

SEXP handle_result(SEXP res_) {
    uintptr_t res = (uintptr_t)res_;

    // An error is indicated by tag.
    if ((res & TAGGED_POINTER_MASK) == 1) {
        // Remove tag
        SEXP res_aligned = (SEXP)(res & ~TAGGED_POINTER_MASK);

        // Currently, there are two types of error cases:
        //
        //   1. Error from Rust code
        //   2. Error from R's C API, which is caught by R_UnwindProtect()
        //
        if (TYPEOF(res_aligned) == CHARSXP) {
            // In case 1, the result is an error message that can be passed to
            // Rf_errorcall() directly.
            Rf_errorcall(R_NilValue, "%s", CHAR(res_aligned));
        } else {
            // In case 2, the result is the token to restart the
            // cleanup process on R's side.
            R_ContinueUnwind(res_aligned);
        }
    }

    return (SEXP)res;
}

SEXP savvy_ffi_option_names__impl(void) {
    SEXP res = savvy_ffi_option_names__ffi();
    return handle_result(res);
}

SEXP savvy_ffi_read_raw__impl(SEXP c_arg__path) {
    SEXP res = savvy_ffi_read_raw__ffi(c_arg__path);
    return handle_result(res);
}

SEXP savvy_ffi_run_deck__impl(SEXP c_arg__deck, SEXP c_arg__is_path, SEXP c_arg__include_names, SEXP c_arg__include_texts, SEXP c_arg__opt_names, SEXP c_arg__opt_values) {
    SEXP res = savvy_ffi_run_deck__ffi(c_arg__deck, c_arg__is_path, c_arg__include_names, c_arg__include_texts, c_arg__opt_names, c_arg__opt_values);
    return handle_result(res);
}

SEXP savvy_ffi_write_raw__impl(SEXP c_arg__path, SEXP c_arg__plots, SEXP c_arg__binary) {
    SEXP res = savvy_ffi_write_raw__ffi(c_arg__path, c_arg__plots, c_arg__binary);
    return handle_result(res);
}


static const R_CallMethodDef CallEntries[] = {
    {"savvy_ffi_option_names__impl", (DL_FUNC) &savvy_ffi_option_names__impl, 0},
    {"savvy_ffi_read_raw__impl", (DL_FUNC) &savvy_ffi_read_raw__impl, 1},
    {"savvy_ffi_run_deck__impl", (DL_FUNC) &savvy_ffi_run_deck__impl, 6},
    {"savvy_ffi_write_raw__impl", (DL_FUNC) &savvy_ffi_write_raw__impl, 3},
    {NULL, NULL, 0}
};

void R_init_spice(DllInfo *dll) {
    R_registerRoutines(dll, NULL, CallEntries, NULL, NULL);
    R_useDynamicSymbols(dll, FALSE);

    // Functions for initialization, if any.

}
