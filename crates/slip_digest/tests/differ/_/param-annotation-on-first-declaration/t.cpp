// std: c++26
#include "side.h"
// §10.5, §5.4 ParamData (WP-100): a parameter's annotations follow its chain: `[[=1]]` on the first of two declarations.
#ifdef B
void f([[=1]] int x);
void f(int x) {}
#else
void f(int x);
void f(int x) {}
#endif
