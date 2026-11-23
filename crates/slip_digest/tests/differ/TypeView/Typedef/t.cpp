#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): a field of an aligned typedef against the type it names (digest-kinds §4.2 rule 9): the field's type digest holds the typedef.
typedef int I __attribute__((aligned(8)));
#ifdef B
struct S { char c; I i; };
#else
struct S { char c; int i; };
#endif
