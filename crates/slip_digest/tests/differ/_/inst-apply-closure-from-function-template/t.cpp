#include "side.h"
// digest §4.3 "in an instantiated body the specialisation": `apply<(lambda)>`
// called from a function template's specialisation. The closure is
// numbered by f<int>, whose body its resolved unit `definition` walks.
template <class F> int apply(F g) { return g(); }
#ifdef B
template <class T> int f() { return apply([] { return 2; }); }
#else
template <class T> int f() { return apply([] { return 1; }); }
#endif
int x = f<int>();
