#include "side.h"
// digest-kinds §11.1 (WP-045 review, N15): Kiln moves f<int> to the
// pattern's definition when it instantiates the definition
// (relocate_to_pattern_definition); the declared unit of the explicit
// instantiation declaration keeps the file of the pattern. The pattern's
// definition is inline, so the declaration does not suppress the
// definition's instantiation (kiln_sema/src/templ/class.rs:1590), and
// Kiln moves f<int> from the explicit instantiation declaration (where
// declare_spec_in put it) to def.h.
#include "decl.h"
#include "def.h"
extern template void f<int>(int);
#ifdef B
void use() { f(1); }
#endif
