#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): a use between an aligned declaration and a plain one: Kiln's `alignof(R)` is 8 there and 4 after the plain one; clang 19 gives 8 for both.
typedef int R __attribute__((aligned(8)));
unsigned long g() { return alignof(R); }
typedef int R;
