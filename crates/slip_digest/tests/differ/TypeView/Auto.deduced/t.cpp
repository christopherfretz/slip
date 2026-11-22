#include "side.h"
// §4.7 TypeView `Auto.deduced`: a templated function's placeholder deduced as dependent (it has a body)
// against the same placeholder undeduced (no body). The body itself is not written (FunctionData `body`).
#ifdef B
template <class T> auto g(T t);
#else
template <class T> auto g(T t) { return t; }
#endif
