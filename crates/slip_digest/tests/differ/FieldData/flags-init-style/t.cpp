#include "side.h"
// §5.5 FieldData `flags-init-style`: `=` against braces.
#ifdef B
struct C { int a{1}; };
#else
struct C { int a = 1; };
#endif
