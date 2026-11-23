#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): a flexible array member of the typedef.
typedef long L __attribute__((aligned(4)));
struct S { char c; L a[]; };
