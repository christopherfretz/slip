#include "side.h"
// §5.3 DeclKind `LifetimeExtendedTemporary`: through the reference it extends: the temporary's initialiser.
#ifdef B
const int &r = 2;
#else
const int &r = 1;
#endif
