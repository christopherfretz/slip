#include "side.h"
// §7.1 ExprDeps `0-unexpanded-pack`.
// the operand of a fold expression (expr_dep.rs)
#ifdef B
template<int... N> int v = (0 + ... + N);
#else
template<int... N> int v = (N + ... + 0);
#endif
