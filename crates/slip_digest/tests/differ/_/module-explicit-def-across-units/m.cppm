module;
#include "side.h"
export module m;
export template <class T> void f(T) {}
#ifndef B
template void f<int>(int);
#endif
