#include "side.h"
// digest-kinds §11.1 (WP-045 review, N15): Kiln moves f<int> to the
// pattern's definition when it instantiates the definition
// (relocate_to_pattern_definition); the declared unit of the explicit
// instantiation declaration keeps the file of the pattern.
#include "decl.h"
#include "def.h"
extern template void f<int>(int);
#ifdef B
void use() { f(1); }
#endif
