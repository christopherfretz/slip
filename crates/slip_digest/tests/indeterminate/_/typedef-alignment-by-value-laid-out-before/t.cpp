#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): aligned(8), a class laid out before the aligned(16) redeclaration: Kiln's `sizeof(S)` is 16 if S is laid out first between them and 32 after (cached); clang 19 gives 16. `g()` and `S` hold no type that meets `R` (`g` holds `sizeof(S)`, which the class's units decide), so only `R` and `S::r` are indeterminate.
typedef int R __attribute__((aligned(8)));
struct S { char c; R r; };
static_assert(sizeof(S) > 0);
typedef int R __attribute__((aligned(16)));
unsigned long g() { return sizeof(S); }
