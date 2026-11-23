#include "side.h"
// digest-kinds §11.9 (WP-032 rule 4): a function with a written
// declaration whose definition an instantiation adds outside its class:
// the definition is `definition`'s, read from Kiln's definition.
int k();
template <class T> struct G { friend int k() { return 2; } };
G<int> gg;
#ifdef B
int h() { return k(); }
#endif
