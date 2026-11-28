#include "side.h"
// WP-138 item 1: binary left fold `(0L + ... + t)` records its
// definition-context operator set in `oper_lookups`.
namespace N { struct X {}; }
#ifdef B
long operator+(long, N::X);
#endif
template<class... T> auto f(T... t) { return (0L + ... + t); }
template<class... T> auto g(T... t) -> decltype((0L + ... + t));
