#include "side.h"
// digest §4.3 "in an instantiated body the specialisation": a lambda in a
// variable template's initialiser, and the variable that calls it.
template <class T> auto v = [] { return 1; };
int x = v<int>();
