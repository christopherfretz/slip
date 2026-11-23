#include "side.h"
// §7 ExprKind `IntegerPackExpr`.
template<class T, T... Is> struct Seq {};
#ifdef B
template<int N> using MS = Seq<int, N>;
#else
template<int N> using MS = Seq<int, __integer_pack(N)...>;
#endif
