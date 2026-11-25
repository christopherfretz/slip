#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md), WP-163: a use between an aligned declaration and a plain one. Since Kiln WP-2830 (`8e23911be`) the plain one carries the first's request (`Ast::self_and_prev`), so `alignof(R)` is the same before and after it (clang 19 too); determinate (digest-kinds §4.2 rule 9). The first declaration's request changed: R's own unit differs.
#ifdef B
typedef int R __attribute__((aligned(16)));
#else
typedef int R __attribute__((aligned(8)));
#endif
unsigned long g() { return alignof(R); }
typedef int R;
