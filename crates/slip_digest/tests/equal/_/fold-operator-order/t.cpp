#include "side.h"
// WP-138 (§9.5 `oper_lookups`, §4.4 "Spellings"): a fold's operator set is
// written sorted by entity, not in declaration order (both full walk and
// `own_spelling`).
namespace N { struct P {}; struct Q {}; }
#ifdef B
N::Q operator+(N::Q, N::Q);
N::P operator+(N::P, N::P);
#else
N::P operator+(N::P, N::P);
N::Q operator+(N::Q, N::Q);
#endif
template<class... T> auto f(T... t) { return (t + ...); }
template<class... T> auto g(T... t) -> decltype((t + ...));
