// std: c++20
#include "side.h"
// §7.4 ColdExprData `use_imm` (symbolic, Kiln WP-3278): the value at a use of an immediate invocation in a default argument holding a source-location builtin; the use moved to another line.
consteval int line(int l = __builtin_LINE()) { return l; }
int g(int v = line()) { return v; }
#ifdef B

int h() { return g(); }
#else
int h() { return g(); }

#endif
