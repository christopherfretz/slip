#include "side.h"
// §7.4 ColdExprData `compound_literal_written` (out, Kiln WP-3589): a compound literal's array type written without its bound, against the bound the list completes.
#ifdef B
int f() { return ((int[]){1, 2})[1]; }
#else
int f() { return ((int[2]){1, 2})[1]; }
#endif
