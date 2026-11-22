#include "side.h"
// §10.1 rule: the encoding `attrs`: the order is kept (two `enable_if` swapped).
#ifdef B
void f(int x) __attribute__((enable_if(x > 1, "b"), enable_if(x > 0, "a")));
#else
void f(int x) __attribute__((enable_if(x > 0, "a"), enable_if(x > 1, "b")));
#endif
