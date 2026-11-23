#include "side.h"
// §7 ExprNode `SubstNonTypeTemplateParmExpr.bits-reference-parameter`: the parameter a reference or not: a member template of the instantiation names the replacement.
constexpr int c = 1;
#ifdef B
template<const int &N> struct A { template<class U> static auto g(U u) -> decltype(u + N); };
auto p = &A<c>::g<int>;
#else
template<int N> struct A { template<class U> static auto g(U u) -> decltype(u + N); };
auto p = &A<1>::g<int>;
#endif
