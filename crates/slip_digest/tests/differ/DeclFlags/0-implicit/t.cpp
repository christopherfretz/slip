#include "side.h"
// §5.3 DeclFlags `0-implicit`: a defaulted default constructor written against the implicit one.
#ifdef B
struct S { S() = default; };
#else
struct S { };
#endif
S s;
