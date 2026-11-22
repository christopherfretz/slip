#include "side.h"
// §10.1 AttrKind: `CFI_CANONICAL_JUMP_TABLE`: the attribute on one side only.
#ifdef B
__attribute__((cfi_canonical_jump_table)) void f();
#else
void f();
#endif
