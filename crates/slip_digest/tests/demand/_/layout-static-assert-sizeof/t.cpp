#include "side.h"
// WP-107 construct (demand, §11.7): `sizeof` in a constant expression the Oven evaluates (`static_assert`): kiln_oven `compile.rs:4479`; every unit stays equal.
struct S { char c; int i; };
#ifdef B
void y() { static_assert(sizeof(S) == 8); }
#endif
