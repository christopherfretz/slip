// std: c++20
#include "side.h"
// ColdExprData `use_imm` (`out`, Kiln WP-3278): the value at a use of an immediate invocation in a default argument holding a source-location builtin is a function of where the use is; the use moved to another line.
consteval int line(int l = __builtin_LINE()) { return l; }
int g(int v = line()) { return v; }
#ifdef B

int h() { return g(); }
#else
int h() { return g(); }

#endif
