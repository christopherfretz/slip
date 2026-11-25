#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md), WP-163: an aligned declaration, then a plain one. Since Kiln WP-2830 (`8e23911be`) each declaration carries its own requests and those before it (`Ast::self_and_prev`), so both have the first's alignment, for `alignof` and layout (clang 19 gives the same); determinate (digest-kinds §4.2 rule 9). The first declaration's request changed: R's own unit differs; `g` and `S` hold R's entity and are decided by its unit (as differ/_/typedef-alignment-value).
#ifdef B
typedef int R __attribute__((aligned(16)));
#else
typedef int R __attribute__((aligned(8)));
#endif
typedef int R;
unsigned long g() { return alignof(R); }
struct S { char c; R r; };
