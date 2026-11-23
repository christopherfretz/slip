#include "side.h"
// digest §4.3 "in an instantiated body the specialisation": `apply<(lambda)>`
// called from a function template's specialisation.
template <class F> int apply(F g) { return g(); }
template <class T> int f() { return apply([] { return 1; }); }
int x = f<int>();
