#include "side.h"
// §5.4 VarSpec `0-exception-var`: the variable of a handler.
#ifdef B
void f() { try {} catch (int e) {} }
#else
void f() { try {} catch (...) { int e; } }
#endif
