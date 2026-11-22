#include "side.h"
// §10.1 AttrKind: `CODE_ALIGN`: the attribute on one side only.
#ifdef B
void f() { [[clang::code_align(16)]] for (;;) { } }
#else
void f() { for (;;) { } }
#endif
