#include "side.h"
// §2.4 (WP-026 addition): a block-scope function declaration writes its own default arguments after its ref.
#ifdef B
void f(int);
void g() { void f(int = 2); }
#else
void f(int);
void g() { void f(int = 1); }
#endif
