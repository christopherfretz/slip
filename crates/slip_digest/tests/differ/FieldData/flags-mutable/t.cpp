#include "side.h"
// §5.5 FieldData `flags-mutable`.
#ifdef B
struct C { mutable int a; };
#else
struct C { int a; };
#endif
