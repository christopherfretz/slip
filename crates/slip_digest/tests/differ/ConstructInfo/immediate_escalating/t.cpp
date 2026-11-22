#include "side.h"
// WP-035, digest-kinds §7 `ConstructInfo` `immediate_escalating`: a consteval constructor called with a non-constant makes the function template's instantiation immediate.
struct I { consteval I(int x) : v(x) {} int v; };
#ifdef B
template <class T> constexpr int f(T t) { return I(1).v; }
int z = f(1);
#else
template <class T> constexpr int f(T t) { return I(t).v; }
int z = f(1);
#endif
