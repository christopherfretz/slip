#include "side.h"
// digest-kinds §11.9 (WP-032 rule 2): a friend function declared only by
// the instantiation of its class; the call instantiates its definition.
template <class T> struct F { friend int g(F) { return 1; } };
F<int> f;
#ifdef B
int h() { return g(f); }
#endif
