#include "side.h"
// §5.7 ColdDeclData `lexical_ctx`: a linkage specification is transparent; a class in one has no lexical context of its own.
#ifdef B
extern "C" { struct S { int i; }; }
#else
struct S { int i; };
#endif
