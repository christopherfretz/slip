#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): a plain declaration, then an aligned one: Kiln's `alignof(R)` is 8; clang 19 gives 8.
typedef int R;
typedef int R __attribute__((aligned(8)));
unsigned long g() { return alignof(R); }
struct S { char c; R r; };
