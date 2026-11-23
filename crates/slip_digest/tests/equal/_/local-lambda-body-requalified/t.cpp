#include "side.h"
// §4.3: what a lambda's body names, requalified.
namespace n { int g(); }
#ifdef B
int f() { return [] { return ::n::g(); }(); }
#else
int f() { return [] { return n::g(); }(); }
#endif
