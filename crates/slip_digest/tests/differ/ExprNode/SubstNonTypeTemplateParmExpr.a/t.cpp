#include "side.h"
// §7 ExprNode `SubstNonTypeTemplateParmExpr.a`: which parameter the instantiation replaced.
#ifdef B
template<int N, int M> struct A { template<class U> static auto g(U u) -> decltype(u + M); };
auto p = &A<3, 3>::g<int>;
#else
template<int N, int M> struct A { template<class U> static auto g(U u) -> decltype(u + N); };
auto p = &A<3, 3>::g<int>;
#endif
