// std: c++26
#include "side.h"
// §7 ExprNode `CXXSpliceExpr.aux`.
// a dependent splice (a non-dependent one is replaced by what it designates)
template<auto R> int v = [:R:];
