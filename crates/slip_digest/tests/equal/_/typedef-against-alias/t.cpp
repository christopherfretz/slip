#include "side.h"
// §5.5 TypedefData: a member typedef against an alias for the same type.
#ifdef B
struct C { using X = int; };
#else
struct C { typedef int X; };
#endif
