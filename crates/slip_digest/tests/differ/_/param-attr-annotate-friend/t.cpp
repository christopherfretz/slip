#include "side.h"
// §10.1, §5.4 ParamData (WP-100): a parameter's chain is parameter i of every declaration of its function: `[[clang::annotate]]` on a friend declaration in a class, defined outside.
#ifdef B
struct S { friend void f([[clang::annotate("x")]] int x); };
void f(int x) {}
#else
struct S { friend void f(int x); };
void f(int x) {}
#endif
