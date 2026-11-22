#include "side.h"
// §5.4 ScopeData `aux-lang`: a function with internal linkage keeps its key; the enclosing language is in its stream.
#ifdef B
extern "C" { static void f(); }
#else
extern "C++" { static void f(); }
#endif
