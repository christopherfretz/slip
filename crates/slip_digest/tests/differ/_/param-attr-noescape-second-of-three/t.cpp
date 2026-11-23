#include "side.h"
// §10.1, §5.4 ParamData (WP-100): a parameter's chain is parameter i of every declaration of its function: `__attribute__((noescape))` on the second of three declarations.
#ifdef B
void f(int *p);
void f(int *p __attribute__((noescape)));
void f(int *p) {}
#else
void f(int *p);
void f(int *p);
void f(int *p) {}
#endif
