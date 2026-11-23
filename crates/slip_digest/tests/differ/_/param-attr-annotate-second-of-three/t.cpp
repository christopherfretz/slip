#include "side.h"
// §10.1, §5.4 ParamData (WP-100): a parameter's chain is parameter i of every declaration of its function: `[[clang::annotate]]` on the second of three declarations.
#ifdef B
void f(int x);
void f([[clang::annotate("x")]] int x);
void f(int x) {}
#else
void f(int x);
void f(int x);
void f(int x) {}
#endif
