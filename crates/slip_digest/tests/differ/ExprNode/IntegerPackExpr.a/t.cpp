#include "side.h"
// §7 ExprNode `IntegerPackExpr.a`: the count.
template<class T, T... Is> struct Seq {};
#ifdef B
template<int N> using MS = Seq<int, __integer_pack(N + 1)...>;
#else
template<int N> using MS = Seq<int, __integer_pack(N)...>;
#endif
