#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): another alignment on the typedef: its own unit; a class with a field of it holds the typedef's entity and is decided by the typedef's unit, as by a field's class type.
#ifdef B
typedef int I __attribute__((aligned(16)));
#else
typedef int I __attribute__((aligned(8)));
#endif
struct S { char c; I i; };
