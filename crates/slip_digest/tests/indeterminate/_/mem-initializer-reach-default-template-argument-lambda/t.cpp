#include "side.h"
// WP-124 fix round F3: a local class in a lambda in a pattern; the cause lands on the unit that holds the lambda, a class template's default template argument (kiln/pattern-mem-initializers).
#ifdef B
#define X 2
#else
#define X 1
#endif
template<class T, int N = []{ struct L { int m; L() : m(X) {} }; return L().m; }()> struct Q { };
