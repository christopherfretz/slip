#include "side.h"
// §4.7 TypeView `Auto.deduced`: two placeholders deduced as dependent, `auto` against `decltype(auto)`.
#ifdef B
template <class T> decltype(auto) g(T t) { return t; }
#else
template <class T> auto g(T t) { return t; }
#endif
