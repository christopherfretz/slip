#include "side.h"
// WP-138 item 1: a non-member operator declared after a fold template
// (outside any operand namespace) is not in `oper_lookups`, so the pattern
// is determinate and unchanged.
namespace N { struct S {}; }
template<class... T> auto f(T... t) { return (t + ...); }
template<class... T> auto g(T... t) -> decltype((t + ...));
#ifdef B
N::S operator+(N::S, N::S);
#endif
