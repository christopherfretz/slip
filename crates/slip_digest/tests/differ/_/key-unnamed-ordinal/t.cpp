#include "side.h"
// digest §3.3: an unnamed bit-field no other rule names is keyed by its
// ordinal among the class's unnamed fields.
#ifdef B
struct S { int a; int : 3; int : 4; };
#else
struct S { int a; int : 3; };
#endif
