#include "side.h"
// WP-138: a fold rebuilt by substitution keeps its pattern's definition-context
// operator set (`fold_combine` / `oper_override`): `operator+` declared after
// `S`'s definition and before `S<N::X>` is not in the rebuilt fold's set, so
// `S<N::X>` and `S<N::X>::f` are unchanged.
namespace N { struct X {}; }
template<class U> struct S { template<class... T> static auto f(T... t) -> decltype((U{} + ... + t)); };
#ifdef B
N::X operator+(N::X, N::X);
#endif
S<N::X> s;
