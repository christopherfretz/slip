// std: c++26
#include "side.h"
// §7 ExprKind `CXXSpliceExpr`.
// a dependent splice (a non-dependent one is replaced by what it designates)
template<auto R> int v = [:R:];
