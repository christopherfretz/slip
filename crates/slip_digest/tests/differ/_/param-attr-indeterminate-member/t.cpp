// std: c++26
#include "side.h"
// §10.1, §5.4 ParamData (WP-100): a parameter's chain is parameter i of every declaration of its function: `[[indeterminate]]` on a member function declared in its class and defined outside.
#ifdef B
struct S { void f([[indeterminate]] int x); };
void S::f(int x) {}
#else
struct S { void f(int x); };
void S::f(int x) {}
#endif
