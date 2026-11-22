#include "side.h"
// §5.5 FieldData `init`: a default member initialiser.
#ifdef B
struct C { int a = 1; };
#else
struct C { int a; };
#endif
