#include "side.h"
// §7 ExprKind `SubstNonTypeTemplateParmExpr`: the pattern names the parameter, the instantiation its replacement.
#ifdef B
template<int N> struct A { template<class U> static auto g(U u) -> decltype(u + 3); };
auto p = &A<3>::g<int>;
#else
template<int N> struct A { template<class U> static auto g(U u) -> decltype(u + N); };
auto p = &A<3>::g<int>;
#endif
