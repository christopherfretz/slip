#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): an array of a typedef that lowers alignment (an over-aligned element is an error): Kiln's layout reads the element's typedef.
typedef long L __attribute__((aligned(4)));
struct S { char c; L a[2]; };
