#include "side.h"
// digest §4.3 "in an instantiated body the specialisation": a lambda in a
// variable template's initialiser, and the variable that calls it. The
// closure is numbered by v<int>, whose initialiser its resolved unit
// `definition` walks.
#ifdef B
template <class T> auto v = [] { return 2; };
#else
template <class T> auto v = [] { return 1; };
#endif
int x = v<int>();
