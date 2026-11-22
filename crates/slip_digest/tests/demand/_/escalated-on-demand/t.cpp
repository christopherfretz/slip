#include "side.h"
// §11.9 (WP-026: a row §11.9 must gain): `0-escalated` is set when the body of an instantiation is analysed; `decltype` names the specialisation without it.
consteval int id(int x) { return x; }
template <class T> constexpr int g(T t) { return id(t); }
using X = decltype(g(1));
#ifdef B
int v() { return g(1); }
#else

#endif
