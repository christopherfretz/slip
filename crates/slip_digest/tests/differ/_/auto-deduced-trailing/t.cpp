#include "side.h"
// §4.7 TypeView `Auto.deduced`: a placeholder deduced as dependent against a trailing `-> int`.
#ifdef B
template <class T> auto g(T t) -> int { return t; }
#else
template <class T> auto g(T t) { return t; }
#endif
