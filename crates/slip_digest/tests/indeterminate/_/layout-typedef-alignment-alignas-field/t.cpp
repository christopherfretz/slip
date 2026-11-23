#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): `alignas(type-id)` on a field naming the typedef.
typedef int I __attribute__((aligned(8)));
struct S { char c; alignas(I) char d; };
