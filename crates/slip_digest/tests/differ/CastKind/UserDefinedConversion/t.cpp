#include "side.h"
// §7.2 CastKind `UserDefinedConversion`.
struct Cv { operator int() const; };
Cv cv; int a;
#ifdef B
int v = a;
#else
int v = cv;
#endif
