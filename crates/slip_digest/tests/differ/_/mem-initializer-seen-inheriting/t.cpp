#include "side.h"
// WP-124 fix round F1: as mem-initializer-seen-instantiated, the base's constructor reached through an inheriting constructor.
#ifdef B
#define X 2
#else
#define X 1
#endif
template<class T> struct Bs { int m; Bs(int) : m(X) { } };
template<class T> struct D : Bs<T> { using Bs<T>::Bs; };
D<int> d(0);
