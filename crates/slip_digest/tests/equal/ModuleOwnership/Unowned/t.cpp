// std: c++20
#include "side.h"
// §5.3 ModuleOwnership `Unowned`: every declaration Kiln makes (no other value is set, kiln_ast/src/decl.rs:512).
export module m;
#ifdef B
int x;
#else
int x;
#endif
