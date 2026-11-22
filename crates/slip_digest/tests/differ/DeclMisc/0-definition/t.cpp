#include "side.h"
// §5.3 DeclMisc `0-definition`.
#ifdef B
struct S {};
#else
struct S;
#endif
