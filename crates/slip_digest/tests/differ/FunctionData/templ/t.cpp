#include "side.h"
// §5.4 FunctionData `templ`, §9.1: the specialisation's template link.
#ifdef B
template <class T> void f(T);
template <> void f<int>(int) {}
#else
template <class T> void f(T);
template <> void f(int) {}
#endif
