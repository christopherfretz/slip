#include "side.h"
// §2.3, §4.3: a default argument taken from another declaration names that declaration's parameters, which are the parameters of the declaration read (WP-026).
#ifdef B
void f(int a, int b = sizeof(a));
void f(int a, int b) {}
#else
void f(int a, int b);
void f(int a, int b = sizeof(a)) {}
#endif
