#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): an aligned declaration, then a plain one: Kiln's `alignof(R)` is 4 (it reads the declaration the use names), its layout 8; clang 19 gives 8 for both. `typedef/alignment-by-declaration`.
typedef int R __attribute__((aligned(8)));
typedef int R;
unsigned long g() { return alignof(R); }
struct S { char c; R r; };
