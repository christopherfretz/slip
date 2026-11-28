#include "side.h"
// WP-138 item 1: unary right fold `(t + ...)` records its definition-context
// operator set in `oper_lookups` (body and trailing return type).
namespace N { struct X {}; }
#ifdef B
int operator+(N::X, N::X);
#endif
template<class... T> auto f(T... t) { return (t + ...); }
template<class... T> auto g(T... t) -> decltype((t + ...));
