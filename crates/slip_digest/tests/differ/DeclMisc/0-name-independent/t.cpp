// std: c++26
#include "side.h"
// §5.3 DeclMisc `0-name-independent`: a second member named `_` (P2169); the bit follows from the name and the scope, so no pair flips it alone.
struct S {
#ifdef B
int _; int _;
#else
int _;
#endif
};
