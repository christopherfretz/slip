#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): aligned(16), then aligned(8) twice: the two latest declarations agree, the first does not (Kiln's `redecls_of` walks from the latest), so every declaration is compared.
typedef int R __attribute__((aligned(16)));
typedef int R __attribute__((aligned(8)));
typedef int R __attribute__((aligned(8)));
unsigned long g() { return alignof(R); }
struct S { char c; R r; };
