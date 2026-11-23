#include "side.h"
// digest §4.3 "in an instantiated body the specialisation": a lambda
// returned from a generic lambda's body, called outside it. The closure
// is numbered by the specialisation of the generic call operator.
#ifdef B
auto g = [](auto) { return [] { return 2; }; };
#else
auto g = [](auto) { return [] { return 1; }; };
#endif
int x = g(0)();
