#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): `sizeof` of a class holding one: the field holds the typedef and `f` holds `sizeof(S)`, which the class decides (item 2).
typedef int I __attribute__((aligned(8)));
#ifdef B
struct S { char c; I i; };
#else
struct S { char c; int i; };
#endif
unsigned long f() { return sizeof(S); }
