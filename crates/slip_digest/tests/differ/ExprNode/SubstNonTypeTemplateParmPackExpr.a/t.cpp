#include "side.h"
// §7 ExprNode `SubstNonTypeTemplateParmPackExpr.a`: the parameter pack substituted.
#ifdef B
template<int... Ks> struct S { template<int... Ms> static auto g() -> decltype(((Ms * Ms) + ...)); };
auto p = &S<1, 2>::g<3, 4>;
#else
template<int... Ns> struct S { template<int... Ms> static auto g() -> decltype(((Ns * Ms) + ...)); };
auto p = &S<1, 2>::g<3, 4>;
#endif
