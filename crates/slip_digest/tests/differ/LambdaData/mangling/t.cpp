#include "side.h"
// §10.4 LambdaData `mangling`: a closure in an inline variable's initialiser is numbered in its context.
#ifdef B
inline auto l = [] { return 1; };
#else
auto l = [] { return 1; };
#endif
