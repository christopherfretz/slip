#include "side.h"
// §10.1 (WP-100, hazard F4): a parameter attribute in a nested declarator, here on the parameter of a function-pointer parameter. Clang mangles it into the type (`_Z1fPFvU8noescapePiE`); Kiln keeps it nowhere.
#ifdef B
void f(void (*cb)(int *q __attribute__((noescape)))) {}
#else
void f(void (*cb)(int *q)) {}
#endif
