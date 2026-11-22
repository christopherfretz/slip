#include "side.h"
// §9.5 `defaults` holding a value: an expression argument, visible once the expression area lands.
#ifdef B
template <class T, int N = 2> using Y = T[N];
#else
template <class T, int N = 1> using Y = T[N];
#endif
