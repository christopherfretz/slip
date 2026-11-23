#include "side.h"
// §7 ExprKind `SubstNonTypeTemplateParmPackExpr`.
#ifdef B
template<int... Ns> struct S { template<int... Ms> static auto g() -> decltype(((1 * Ms) + ...)); };
auto p = &S<1, 2>::g<3, 4>;
#else
template<int... Ns> struct S { template<int... Ms> static auto g() -> decltype(((Ns * Ms) + ...)); };
auto p = &S<1, 2>::g<3, 4>;
#endif
