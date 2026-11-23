#include "side.h"
// WP-124 fix round F1: as mem-initializer-seen-instantiated, a constructor template of a class that is not a template, through its specialisation.
#ifdef B
#define X 2
#else
#define X 1
#endif
struct P { int m; template<class U> P(U) : m(X) { } };
P p(1);
