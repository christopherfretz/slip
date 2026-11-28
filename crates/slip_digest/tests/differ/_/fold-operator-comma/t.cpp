#include "side.h"
// WP-138 item 1: `,` fold records its definition-context operator set in
// `oper_lookups`; an empty-pack instantiation produces `void()` without calling
// any operator.
namespace N { struct X {}; }
#ifdef B
N::X operator,(N::X, N::X);
#endif
template<class... T> void f(T... t) { (t, ...); }
template<class... T> auto g(T... t) -> decltype((t, ...));
void call() { f(); }
