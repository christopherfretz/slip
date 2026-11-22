#include "side.h"
// §10.1 AttrNode: `CODE_ALIGN.a`: the fact differs.
#ifdef B
void f() { [[clang::code_align(32)]] for (;;) { } }
#else
void f() { [[clang::code_align(16)]] for (;;) { } }
#endif
