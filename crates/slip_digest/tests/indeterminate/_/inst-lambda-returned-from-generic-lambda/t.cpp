#include "side.h"
// digest §4.3 "in an instantiated body the specialisation": a lambda
// returned from a generic lambda's body, called outside it.
auto g = [](auto) { return [] { return 1; }; };
int x = g(0)();
