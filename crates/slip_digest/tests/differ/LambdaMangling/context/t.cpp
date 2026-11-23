#include "side.h"
// §10.4 LambdaMangling `context`: a closure in an inline variable's initialiser has it.
#ifdef B
inline auto l = [] { return 1; };
#else
auto l = [] { return 1; };
#endif
