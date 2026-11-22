#include "side.h"
// §5.4 ScopeData `aux-braceless`.
#ifdef B
extern "C" void f();
#else
extern "C" { void f(); }
#endif
