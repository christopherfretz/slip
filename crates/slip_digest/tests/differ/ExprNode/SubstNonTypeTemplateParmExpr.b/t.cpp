#include "side.h"
// §7 ExprNode `SubstNonTypeTemplateParmExpr.b`: the replacement.
#ifdef B
template<int N> struct A { template<class U> static auto g(U u) -> decltype(u + N); };
auto p = &A<4>::g<int>;
#else
template<int N> struct A { template<class U> static auto g(U u) -> decltype(u + N); };
auto p = &A<3>::g<int>;
#endif
