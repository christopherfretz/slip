#include "side.h"
// WP-138 item 1: `&&` fold records its definition-context operator set in
// `oper_lookups`; an empty-pack instantiation produces `true` without calling
// any operator.
namespace N { struct X {}; }
#ifdef B
bool operator&&(N::X, N::X);
#endif
template<class... T> bool f(T... t) { return (t && ...); }
template<class... T> auto g(T... t) -> decltype((t && ...));
bool call() { return f(); }
