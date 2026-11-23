// std: c++26
#include "side.h"
// §10.1, §5.4 ParamData (WP-100): a parameter's chain is parameter i of every declaration of its function: `[[indeterminate]]` on the first of two declarations, the definition read.
#ifdef B
void f([[indeterminate]] int x);
void f(int x) { int y = x; }
#else
void f(int x);
void f(int x) { int y = x; }
#endif
