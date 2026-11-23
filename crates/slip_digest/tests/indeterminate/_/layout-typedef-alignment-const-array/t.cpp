#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): an array of a cv-qualified typedef of the typedef.
typedef long L __attribute__((aligned(4)));
typedef const L CL;
struct S { char c; CL a[2]; };
